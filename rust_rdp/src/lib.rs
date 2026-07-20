mod callback;
#[cfg(feature = "android")]
mod android_jni;

pub use callback::{SessionCallback, SharedCallback};

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;
use tokio::time::{sleep, Duration};
use tokio::sync::mpsc;
use tokio_rustls::TlsConnector;
use rustls::client::danger::{ServerCertVerifier, ServerCertVerified, HandshakeSignatureValid};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

use ironrdp_connector::{ClientConnector, Config, Credentials, DesktopSize, BitmapConfig, ServerName as RdpServerName};
use ironrdp_pdu::{Action, Decode, WriteBuf, ReadCursor};
use ironrdp_pdu::fast_path::{FastPathHeader, FastPathUpdatePdu, FastPathUpdate};
use ironrdp_pdu::bitmap::Compression;
use ironrdp_pdu::surface_commands::SurfaceCommand;
use ironrdp_pdu::input::fast_path::{FastPathInput, FastPathInputEvent, KeyboardFlags};
use ironrdp_pdu::input::mouse::PointerFlags;
use ironrdp_pdu::input::MousePdu;
use ironrdp_pdu::geometry::Rectangle;
use ironrdp_graphics::rdp6::BitmapStreamDecoder;
use ironrdp_graphics::rle::RlePixelFormat;
use ironrdp_async::{connect_begin, connect_finalize, mark_as_upgraded, FramedWrite};
use ironrdp_tokio::{TokioFramed, split_tokio_framed};
use ironrdp_dvc::DrdynvcClient;
use ironrdp_egfx::client::{GraphicsPipelineClient, GraphicsPipelineHandler, BitmapUpdate};
use ironrdp_egfx::pdu::{
    CapabilitySet, GfxPdu, WireToSurface2Pdu, SolidFillPdu, SurfaceToSurfacePdu,
    SurfaceToCachePdu, CacheToSurfacePdu, EvictCacheEntryPdu, MapSurfaceToWindowPdu,
    MapSurfaceToScaledOutputPdu, MapSurfaceToScaledWindowPdu, DeleteEncodingContextPdu,
    CacheImportReplyPdu, CapabilitiesV81Flags, CapabilitiesV107Flags, Codec2Type,
};
use ironrdp_pdu::codecs::rfx::progressive::{
    decode_progressive_stream, ProgressiveBlock, ProgressiveTile,
};
use ironrdp_pdu::codecs::rfx::Quant;

use callback::{notify_resolution_change, notify_state_change, push_frame};

enum SessionType {
    Rdp {
        input_tx: mpsc::UnboundedSender<FastPathInputEvent>,
    },
    Vnc {
        input_tx: mpsc::UnboundedSender<vnc::X11Event>,
        button_mask: Arc<Mutex<u8>>,
    },
}

struct RdpSession {
    active: Arc<Mutex<bool>>,
    session_type: SessionType,
    callback: SharedCallback,
}

lazy_static! {
    static ref RUNTIME: Mutex<Option<tokio::runtime::Runtime>> = Mutex::new(None);
    /// Concurrent remote sessions (desktop multi-tab). Android typically uses one.
    static ref SESSIONS: Mutex<HashMap<u64, Arc<Mutex<RdpSession>>>> = Mutex::new(HashMap::new());
}

/// Monotonic session ids starting at 1 (0 = none).
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
/// Session that receives mouse/keyboard input from the single-session input APIs.
static ACTIVE_SESSION_ID: AtomicU64 = AtomicU64::new(0);

fn register_session(session: Arc<Mutex<RdpSession>>) -> u64 {
    let id = NEXT_SESSION_ID.fetch_add(1, AtomicOrdering::SeqCst);
    SESSIONS.lock().unwrap().insert(id, session);
    ACTIVE_SESSION_ID.store(id, AtomicOrdering::SeqCst);
    id
}

fn with_active_session<F>(f: F)
where
    F: FnOnce(&RdpSession),
{
    let id = ACTIVE_SESSION_ID.load(AtomicOrdering::SeqCst);
    if id == 0 {
        return;
    }
    let sessions = SESSIONS.lock().unwrap();
    if let Some(session) = sessions.get(&id) {
        let sess = session.lock().unwrap();
        f(&sess);
    }
}

#[derive(Debug)]
struct NoVerify;

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
        ]
    }
}

struct SimpleNetworkClient;

impl ironrdp_async::NetworkClient for SimpleNetworkClient {
    async fn send(&mut self, _request: &ironrdp_connector::sspi::generator::NetworkRequest) -> ironrdp_connector::ConnectorResult<Vec<u8>> {
        Err(ironrdp_connector::general_err!("SSPI network request not supported"))
    }
}

fn scancode_to_keysym(scancode: u32, _is_extended: bool) -> Option<u32> {
    match scancode {
        // Special Keys
        0x01 => Some(0xff1b), // Escape
        0x0f => Some(0xff09), // Tab
        0x0e => Some(0xff08), // Backspace
        0x1c => Some(0xff0d), // Return
        0x1d => Some(0xffe3), // Control_L
        0x38 => Some(0xffe9), // Alt_L
        0x2a => Some(0xffe1), // Shift_L
        0x36 => Some(0xffe2), // Shift_R
        0x5b => Some(0xffeb), // Super_L (Win)
        0x3a => Some(0xffe5), // Caps Lock
        0x53 => Some(0xffff), // Delete
        0x52 => Some(0xff63), // Insert
        0x37 => Some(0xff61), // Print Screen
        0x47 => Some(0xff50), // Home
        0x4f => Some(0xff57), // End
        0x49 => Some(0xff55), // Page Up
        0x51 => Some(0xff56), // Page Down
        
        // Arrows
        0x4b => Some(0xff51), // Left
        0x48 => Some(0xff52), // Up
        0x50 => Some(0xff54), // Down
        0x4d => Some(0xff53), // Right

        // Numbers
        0x02 => Some(0x31), // 1
        0x03 => Some(0x32), // 2
        0x04 => Some(0x33), // 3
        0x05 => Some(0x34), // 4
        0x06 => Some(0x35), // 5
        0x07 => Some(0x36), // 6
        0x08 => Some(0x37), // 7
        0x09 => Some(0x38), // 8
        0x0a => Some(0x39), // 9
        0x0b => Some(0x30), // 0
        0x0c => Some(0x2d), // -
        0x0d => Some(0x3d), // =

        // Letters (Lowercase keysyms as default, VNC server applies Shift modifier on its side)
        0x10 => Some(0x71), // Q
        0x11 => Some(0x77), // W
        0x12 => Some(0x65), // E
        0x13 => Some(0x72), // R
        0x14 => Some(0x74), // T
        0x15 => Some(0x79), // Y
        0x16 => Some(0x75), // U
        0x17 => Some(0x69), // I
        0x18 => Some(0x6f), // O
        0x19 => Some(0x70), // P
        
        0x1e => Some(0x61), // A
        0x1f => Some(0x73), // S
        0x20 => Some(0x64), // D
        0x21 => Some(0x66), // F
        0x22 => Some(0x67), // G
        0x23 => Some(0x68), // H
        0x24 => Some(0x6a), // J
        0x25 => Some(0x6b), // K
        0x26 => Some(0x6c), // L
        
        0x2c => Some(0x7a), // Z
        0x2d => Some(0x78), // X
        0x2e => Some(0x63), // C
        0x2f => Some(0x76), // V
        0x30 => Some(0x62), // B
        0x31 => Some(0x6e), // N
        0x32 => Some(0x6d), // M

        // Symbols
        0x1a => Some(0x5b), // [
        0x1b => Some(0x5d), // ]
        0x2b => Some(0x5c), // \
        0x27 => Some(0x3b), // ;
        0x28 => Some(0x27), // '
        0x29 => Some(0x60), // `
        0x33 => Some(0x2c), // ,
        0x34 => Some(0x2e), // .
        0x35 => Some(0x2f), // /
        0x39 => Some(0x20), // Space

        // Function Keys
        0x3b => Some(0xffbe), // F1
        0x3c => Some(0xffbf), // F2
        0x3d => Some(0xffc0), // F3
        0x3e => Some(0xffc1), // F4
        0x3f => Some(0xffc2), // F5
        0x40 => Some(0xffc3), // F6
        0x41 => Some(0xffc4), // F7
        0x42 => Some(0xffc5), // F8
        0x43 => Some(0xffc6), // F9
        0x44 => Some(0xffc7), // F10
        0x57 => Some(0xffc8), // F11
        0x58 => Some(0xffc9), // F12

        _ => None,
    }
}

fn copy_vnc_rect_to_screen(
    screen_pixels: &mut [i32],
    screen_w: i32,
    screen_h: i32,
    rect: &vnc::Rect,
    decoded_data: &[u8],
) {
    let rect_w = rect.width as i32;
    let rect_h = rect.height as i32;
    
    for dy in 0..rect_h {
        let dest_y = rect.y as i32 + dy;
        if dest_y < 0 || dest_y >= screen_h {
            continue;
        }
        
        for dx in 0..rect_w {
            let dest_x = rect.x as i32 + dx;
            if dest_x < 0 || dest_x >= screen_w {
                continue;
            }
            
            let dest_idx = (dest_y * screen_w + dest_x) as usize;
            let src_pixel_idx = (dy * rect_w + dx) as usize;
            
            if src_pixel_idx * 4 + 3 < decoded_data.len() {
                let b = decoded_data[src_pixel_idx * 4];
                let g = decoded_data[src_pixel_idx * 4 + 1];
                let r = decoded_data[src_pixel_idx * 4 + 2];
                let a = decoded_data[src_pixel_idx * 4 + 3];
                screen_pixels[dest_idx] = (((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32;
            }
        }
    }
}

fn copy_vnc_screen_rect(
    screen_pixels: &mut [i32],
    screen_w: i32,
    screen_h: i32,
    dst: &vnc::Rect,
    src: &vnc::Rect,
) {
    let rect_w = dst.width as i32;
    let rect_h = dst.height as i32;
    
    let y_range: Vec<i32> = (0..rect_h).collect();
    let y_iterator: Box<dyn Iterator<Item = i32>> = if dst.y > src.y {
        Box::new(y_range.into_iter().rev())
    } else {
        Box::new(y_range.into_iter())
    };

    for dy in y_iterator {
        let src_y = src.y as i32 + dy;
        let dest_y = dst.y as i32 + dy;
        if src_y < 0 || src_y >= screen_h || dest_y < 0 || dest_y >= screen_h {
            continue;
        }

        let x_range: Vec<i32> = (0..rect_w).collect();
        let x_iterator: Box<dyn Iterator<Item = i32>> = if dst.x > src.x {
            Box::new(x_range.into_iter().rev())
        } else {
            Box::new(x_range.into_iter())
        };

        for dx in x_iterator {
            let src_x = src.x as i32 + dx;
            let dest_x = dst.x as i32 + dx;
            if src_x < 0 || src_x >= screen_w || dest_x < 0 || dest_x >= screen_w {
                continue;
            }

            let src_idx = (src_y * screen_w + src_x) as usize;
            let dest_idx = (dest_y * screen_w + dest_x) as usize;
            screen_pixels[dest_idx] = screen_pixels[src_idx];
        }
    }
}

fn copy_gfx_bitmap_to_screen(
    screen_pixels: &mut [i32],
    screen_w: i32,
    screen_h: i32,
    rect: &ironrdp_pdu::geometry::InclusiveRectangle,
    decoded_data: &[u8],
) {
    let rect_w = rect.width() as i32;
    let rect_h = rect.height() as i32;
    
    for dy in 0..rect_h {
        let dest_y = rect.top as i32 + dy;
        if dest_y < 0 || dest_y >= screen_h {
            continue;
        }
        
        for dx in 0..rect_w {
            let dest_x = rect.left as i32 + dx;
            if dest_x < 0 || dest_x >= screen_w {
                continue;
            }
            
            let dest_idx = (dest_y * screen_w + dest_x) as usize;
            let src_pixel_idx = (dy * rect_w + dx) as usize;
            
            if src_pixel_idx * 4 + 3 < decoded_data.len() {
                let r = decoded_data[src_pixel_idx * 4];
                let g = decoded_data[src_pixel_idx * 4 + 1];
                let b = decoded_data[src_pixel_idx * 4 + 2];
                let a = decoded_data[src_pixel_idx * 4 + 3];
                screen_pixels[dest_idx] = (((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32;
            }
        }
    }
}

fn copy_tile_to_screen(
    screen_pixels: &mut [i32],
    screen_w: i32,
    screen_h: i32,
    tile_x: i32,
    tile_y: i32,
    tile_rgba: &[u8],
) {
    for dy in 0..64 {
        let dest_y = tile_y + dy;
        if dest_y < 0 || dest_y >= screen_h {
            continue;
        }
        for dx in 0..64 {
            let dest_x = tile_x + dx;
            if dest_x < 0 || dest_x >= screen_w {
                continue;
            }
            let dest_idx = (dest_y * screen_w + dest_x) as usize;
            let src_pixel_idx = (dy * 64 + dx) as usize;
            if src_pixel_idx * 4 + 3 < tile_rgba.len() {
                let r = tile_rgba[src_pixel_idx * 4];
                let g = tile_rgba[src_pixel_idx * 4 + 1];
                let b = tile_rgba[src_pixel_idx * 4 + 2];
                let a = tile_rgba[src_pixel_idx * 4 + 3];
                screen_pixels[dest_idx] = (((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32;
            }
        }
    }
}

fn to_quant(ccq: &ironrdp_pdu::codecs::rfx::progressive::ComponentCodecQuant) -> Quant {
    Quant {
        ll3: ccq.ll3,
        lh3: ccq.lh3,
        hl3: ccq.hl3,
        hh3: ccq.hh3,
        lh2: ccq.lh2,
        hl2: ccq.hl2,
        hh2: ccq.hh2,
        lh1: ccq.lh1,
        hl1: ccq.hl1,
        hh1: ccq.hh1,
    }
}

fn decode_progressive_stream_to_screen(
    bitmap_data: &[u8],
    screen_pixels: &mut [i32],
    screen_w: i32,
    screen_h: i32,
) -> Result<(), String> {
    let blocks = decode_progressive_stream(bitmap_data)
        .map_err(|e| format!("decode_progressive_stream error: {:?}", e))?;

    for block in blocks {
        if let ProgressiveBlock::Region(region) = block {
            for tile in &region.tiles {
                match tile {
                    ProgressiveTile::Simple(t) => {
                        let mut y_coeffs = [0i16; 4096];
                        let mut cb_coeffs = [0i16; 4096];
                        let mut cr_coeffs = [0i16; 4096];
                        let mut temp = [0i16; 4096];

                        let q_y = region.quant_vals.get(t.quant_idx_y as usize)
                            .ok_or_else(|| "Quant index Y out of range".to_string())?;
                        let q_cb = region.quant_vals.get(t.quant_idx_cb as usize)
                            .ok_or_else(|| "Quant index Cb out of range".to_string())?;
                        let q_cr = region.quant_vals.get(t.quant_idx_cr as usize)
                            .ok_or_else(|| "Quant index Cr out of range".to_string())?;

                        ironrdp_graphics::rlgr::decode(
                            ironrdp_pdu::codecs::rfx::EntropyAlgorithm::Rlgr1,
                            t.y_data,
                            &mut y_coeffs,
                        ).map_err(|e| format!("RLGR Y decode failed: {:?}", e))?;

                        ironrdp_graphics::rlgr::decode(
                            ironrdp_pdu::codecs::rfx::EntropyAlgorithm::Rlgr1,
                            t.cb_data,
                            &mut cb_coeffs,
                        ).map_err(|e| format!("RLGR Cb decode failed: {:?}", e))?;

                        ironrdp_graphics::rlgr::decode(
                            ironrdp_pdu::codecs::rfx::EntropyAlgorithm::Rlgr1,
                            t.cr_data,
                            &mut cr_coeffs,
                        ).map_err(|e| format!("RLGR Cr decode failed: {:?}", e))?;

                        ironrdp_graphics::subband_reconstruction::decode(&mut y_coeffs[4032..]);
                        ironrdp_graphics::subband_reconstruction::decode(&mut cb_coeffs[4032..]);
                        ironrdp_graphics::subband_reconstruction::decode(&mut cr_coeffs[4032..]);

                        ironrdp_graphics::quantization::decode(&mut y_coeffs, &to_quant(q_y));
                        ironrdp_graphics::quantization::decode(&mut cb_coeffs, &to_quant(q_cb));
                        ironrdp_graphics::quantization::decode(&mut cr_coeffs, &to_quant(q_cr));

                        if region.uses_reduce_extrapolate() {
                            ironrdp_graphics::dwt_extrapolate::decode(&mut y_coeffs, &mut temp);
                            ironrdp_graphics::dwt_extrapolate::decode(&mut cb_coeffs, &mut temp);
                            ironrdp_graphics::dwt_extrapolate::decode(&mut cr_coeffs, &mut temp);
                        } else {
                            ironrdp_graphics::dwt::decode(&mut y_coeffs, &mut temp);
                            ironrdp_graphics::dwt::decode(&mut cb_coeffs, &mut temp);
                            ironrdp_graphics::dwt::decode(&mut cr_coeffs, &mut temp);
                        }

                        let mut rgba_buf = [0u8; 64 * 64 * 4];
                        let ycbcr_buf = ironrdp_graphics::color_conversion::YCbCrBuffer {
                            y: &y_coeffs,
                            cb: &cb_coeffs,
                            cr: &cr_coeffs,
                        };
                        ironrdp_graphics::color_conversion::ycbcr_to_rgba(ycbcr_buf, &mut rgba_buf)
                            .map_err(|e| format!("ycbcr_to_rgba failed: {:?}", e))?;

                        copy_tile_to_screen(
                            screen_pixels,
                            screen_w,
                            screen_h,
                            (t.x_idx * 64) as i32,
                            (t.y_idx * 64) as i32,
                            &rgba_buf,
                        );
                    }
                    ProgressiveTile::First(t) => {
                        let mut y_coeffs = [0i16; 4096];
                        let mut cb_coeffs = [0i16; 4096];
                        let mut cr_coeffs = [0i16; 4096];
                        let mut temp = [0i16; 4096];

                        let q_y = region.quant_vals.get(t.quant_idx_y as usize)
                            .ok_or_else(|| "Quant index Y out of range".to_string())?;
                        let q_cb = region.quant_vals.get(t.quant_idx_cb as usize)
                            .ok_or_else(|| "Quant index Cb out of range".to_string())?;
                        let q_cr = region.quant_vals.get(t.quant_idx_cr as usize)
                            .ok_or_else(|| "Quant index Cr out of range".to_string())?;

                        ironrdp_graphics::rlgr::decode(
                            ironrdp_pdu::codecs::rfx::EntropyAlgorithm::Rlgr1,
                            t.y_data,
                            &mut y_coeffs,
                        ).map_err(|e| format!("RLGR Y decode failed: {:?}", e))?;

                        ironrdp_graphics::rlgr::decode(
                            ironrdp_pdu::codecs::rfx::EntropyAlgorithm::Rlgr1,
                            t.cb_data,
                            &mut cb_coeffs,
                        ).map_err(|e| format!("RLGR Cb decode failed: {:?}", e))?;

                        ironrdp_graphics::rlgr::decode(
                            ironrdp_pdu::codecs::rfx::EntropyAlgorithm::Rlgr1,
                            t.cr_data,
                            &mut cr_coeffs,
                        ).map_err(|e| format!("RLGR Cr decode failed: {:?}", e))?;

                        ironrdp_graphics::subband_reconstruction::decode(&mut y_coeffs[4032..]);
                        ironrdp_graphics::subband_reconstruction::decode(&mut cb_coeffs[4032..]);
                        ironrdp_graphics::subband_reconstruction::decode(&mut cr_coeffs[4032..]);

                        ironrdp_graphics::quantization::decode(&mut y_coeffs, &to_quant(q_y));
                        ironrdp_graphics::quantization::decode(&mut cb_coeffs, &to_quant(q_cb));
                        ironrdp_graphics::quantization::decode(&mut cr_coeffs, &to_quant(q_cr));

                        if region.uses_reduce_extrapolate() {
                            ironrdp_graphics::dwt_extrapolate::decode(&mut y_coeffs, &mut temp);
                            ironrdp_graphics::dwt_extrapolate::decode(&mut cb_coeffs, &mut temp);
                            ironrdp_graphics::dwt_extrapolate::decode(&mut cr_coeffs, &mut temp);
                        } else {
                            ironrdp_graphics::dwt::decode(&mut y_coeffs, &mut temp);
                            ironrdp_graphics::dwt::decode(&mut cb_coeffs, &mut temp);
                            ironrdp_graphics::dwt::decode(&mut cr_coeffs, &mut temp);
                        }

                        let mut rgba_buf = [0u8; 64 * 64 * 4];
                        let ycbcr_buf = ironrdp_graphics::color_conversion::YCbCrBuffer {
                            y: &y_coeffs,
                            cb: &cb_coeffs,
                            cr: &cr_coeffs,
                        };
                        ironrdp_graphics::color_conversion::ycbcr_to_rgba(ycbcr_buf, &mut rgba_buf)
                            .map_err(|e| format!("ycbcr_to_rgba failed: {:?}", e))?;

                        copy_tile_to_screen(
                            screen_pixels,
                            screen_w,
                            screen_h,
                            (t.x_idx * 64) as i32,
                            (t.y_idx * 64) as i32,
                            &rgba_buf,
                        );
                    }
                    ProgressiveTile::Upgrade(_) => {
                        // Ignore refinement updates for now, keeping simple/first tile
                    }
                }
            }
        }
    }

    Ok(())
}

struct MyGfxHandler {
    callback: SharedCallback,
    screen_pixels: Arc<Mutex<Vec<i32>>>,
    width: i32,
    height: i32,
}
impl GraphicsPipelineHandler for MyGfxHandler {
    fn capabilities(&self) -> Vec<CapabilitySet> {
        vec![
            CapabilitySet::V10_7 {
                flags: CapabilitiesV107Flags::SMALL_CACHE | CapabilitiesV107Flags::AVC_THIN_CLIENT,
            },
            CapabilitySet::V8_1 {
                flags: CapabilitiesV81Flags::AVC420_ENABLED | CapabilitiesV81Flags::SMALL_CACHE,
            },
        ]
    }

    fn on_capabilities_confirmed(&mut self, caps: &CapabilitySet) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] Capabilities confirmed: {:?}", caps));
    }

    fn on_reset_graphics(&mut self, width: u32, height: u32) {
        let w = width as i32;
        let h = height as i32;
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_reset_graphics received: {}x{}", w, h));
        
        self.width = w;
        self.height = h;
        
        let mut pixels = self.screen_pixels.lock().unwrap();
        pixels.resize((w * h) as usize, 0);
        
        notify_resolution_change(self.callback.as_ref(), w, h);
    }

    fn on_bitmap_updated(&mut self, update: &BitmapUpdate) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_bitmap_updated: rect=({:?}), data={}", update.destination_rectangle, update.data.len()));
        let mut pixels = self.screen_pixels.lock().unwrap();
        copy_gfx_bitmap_to_screen(
            &mut pixels,
            self.width,
            self.height,
            &update.destination_rectangle,
            &update.data,
        );
    }

    fn on_frame_complete(&mut self, frame_id: u32) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_frame_complete: frame_id={}", frame_id));
        let pixels = self.screen_pixels.lock().unwrap();
        push_frame(self.callback.as_ref(), &pixels, self.width, self.height);
    }

    fn on_wire_to_surface2(&mut self, pdu: &WireToSurface2Pdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_wire_to_surface2: codec_id={:?}, data_len={}", pdu.codec_id, pdu.bitmap_data.len()));
        if pdu.codec_id == Codec2Type::RemoteFxProgressive {
            let mut pixels = self.screen_pixels.lock().unwrap();
            if let Err(e) = decode_progressive_stream_to_screen(&pdu.bitmap_data, &mut pixels, self.width, self.height) {
                notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] progressive decode error: {:?}", e));
            }
        }
    }

    fn on_solid_fill(&mut self, pdu: &SolidFillPdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_solid_fill: surface_id={}, color={:?}, rects={}", pdu.surface_id, pdu.fill_pixel, pdu.rectangles.len()));
    }

    fn on_surface_to_surface(&mut self, pdu: &SurfaceToSurfacePdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_surface_to_surface: src={}, dest={}", pdu.source_surface_id, pdu.destination_surface_id));
    }

    fn on_surface_to_cache(&mut self, pdu: &SurfaceToCachePdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_surface_to_cache: surface_id={}, slot={}", pdu.surface_id, pdu.cache_slot));
    }

    fn on_cache_to_surface(&mut self, pdu: &CacheToSurfacePdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_cache_to_surface: surface_id={}, slot={}", pdu.surface_id, pdu.cache_slot));
    }

    fn on_evict_cache_entry(&mut self, pdu: &EvictCacheEntryPdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_evict_cache_entry: slot={}", pdu.cache_slot));
    }

    fn on_map_surface_to_window(&mut self, pdu: &MapSurfaceToWindowPdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_map_surface_to_window: surface_id={}, window_id={}", pdu.surface_id, pdu.window_id));
    }

    fn on_map_surface_to_scaled_output(&mut self, pdu: &MapSurfaceToScaledOutputPdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_map_surface_to_scaled_output: surface_id={}", pdu.surface_id));
    }

    fn on_map_surface_to_scaled_window(&mut self, pdu: &MapSurfaceToScaledWindowPdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_map_surface_to_scaled_window: surface_id={}", pdu.surface_id));
    }

    fn on_delete_encoding_context(&mut self, pdu: &DeleteEncodingContextPdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_delete_encoding_context: surface_id={}", pdu.surface_id));
    }

    fn on_cache_import_reply(&mut self, pdu: &CacheImportReplyPdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_cache_import_reply: slots={}", pdu.cache_slots.len()));
    }

    fn on_unhandled_pdu(&mut self, pdu: &GfxPdu) {
        notify_state_change(self.callback.as_ref(), 2, &format!("[Rust Log] on_unhandled_pdu: {:?}", pdu));
    }
}

fn copy_bitmap_to_screen(
    screen_pixels: &mut [i32],
    screen_w: i32,
    screen_h: i32,
    rect: &ironrdp_pdu::geometry::InclusiveRectangle,
    decoded_data: &[u8],
    bpp: usize,
    format: RlePixelFormat,
) {
    let rect_w = rect.width() as i32;
    let rect_h = rect.height() as i32;
    
    for dy in 0..rect_h {
        let dest_y = rect.top as i32 + (rect_h - 1 - dy);
        if dest_y < 0 || dest_y >= screen_h {
            continue;
        }
        
        for dx in 0..rect_w {
            let dest_x = rect.left as i32 + dx;
            if dest_x < 0 || dest_x >= screen_w {
                continue;
            }
            
            let dest_idx = (dest_y * screen_w + dest_x) as usize;
            let src_pixel_idx = (dy * rect_w + dx) as usize;
            
            let pixel_color = match format {
                RlePixelFormat::Rgb24 => {
                    if src_pixel_idx * 3 + 2 >= decoded_data.len() { continue; }
                    let r = decoded_data[src_pixel_idx * 3];
                    let g = decoded_data[src_pixel_idx * 3 + 1];
                    let b = decoded_data[src_pixel_idx * 3 + 2];
                    ((0xffu32 << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32
                }
                RlePixelFormat::Rgb16 => {
                    if src_pixel_idx * 2 + 1 >= decoded_data.len() { continue; }
                    let val = u16::from_le_bytes([
                        decoded_data[src_pixel_idx * 2],
                        decoded_data[src_pixel_idx * 2 + 1],
                    ]);
                    let r = ((val >> 11) & 0x1F) << 3;
                    let g = ((val >> 5) & 0x3F) << 2;
                    let b = (val & 0x1F) << 3;
                    ((0xffu32 << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32
                }
                RlePixelFormat::Rgb15 => {
                    if src_pixel_idx * 2 + 1 >= decoded_data.len() { continue; }
                    let val = u16::from_le_bytes([
                        decoded_data[src_pixel_idx * 2],
                        decoded_data[src_pixel_idx * 2 + 1],
                    ]);
                    let r = ((val >> 10) & 0x1F) << 3;
                    let g = ((val >> 5) & 0x1F) << 3;
                    let b = (val & 0x1F) << 3;
                    ((0xffu32 << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32
                }
                RlePixelFormat::Rgb8 => {
                    if bpp == 32 {
                        if src_pixel_idx * 4 + 2 >= decoded_data.len() { continue; }
                        let b = decoded_data[src_pixel_idx * 4];
                        let g = decoded_data[src_pixel_idx * 4 + 1];
                        let r = decoded_data[src_pixel_idx * 4 + 2];
                        ((0xffu32 << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32
                    } else {
                        if src_pixel_idx >= decoded_data.len() { continue; }
                        let val = decoded_data[src_pixel_idx] as u32;
                        ((0xffu32 << 24) | (val << 16) | (val << 8) | val) as i32
                    }
                }
            };
            
            screen_pixels[dest_idx] = pixel_color;
        }
    }
}

fn copy_surface_to_screen(
    screen_pixels: &mut [i32],
    screen_w: i32,
    screen_h: i32,
    rect: &ironrdp_pdu::geometry::ExclusiveRectangle,
    decoded_data: &[u8],
    bpp: usize,
    format: RlePixelFormat,
) {
    let rect_w = rect.width() as i32;
    let rect_h = rect.height() as i32;
    
    for dy in 0..rect_h {
        let dest_y = rect.top as i32 + dy;
        if dest_y < 0 || dest_y >= screen_h {
            continue;
        }
        
        for dx in 0..rect_w {
            let dest_x = rect.left as i32 + dx;
            if dest_x < 0 || dest_x >= screen_w {
                continue;
            }
            
            let dest_idx = (dest_y * screen_w + dest_x) as usize;
            let src_pixel_idx = (dy * rect_w + dx) as usize;
            
            let pixel_color = match format {
                RlePixelFormat::Rgb24 => {
                    if src_pixel_idx * 3 + 2 >= decoded_data.len() { continue; }
                    let r = decoded_data[src_pixel_idx * 3];
                    let g = decoded_data[src_pixel_idx * 3 + 1];
                    let b = decoded_data[src_pixel_idx * 3 + 2];
                    ((0xffu32 << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32
                }
                RlePixelFormat::Rgb16 => {
                    if src_pixel_idx * 2 + 1 >= decoded_data.len() { continue; }
                    let val = u16::from_le_bytes([
                        decoded_data[src_pixel_idx * 2],
                        decoded_data[src_pixel_idx * 2 + 1],
                    ]);
                    let r = ((val >> 11) & 0x1F) << 3;
                    let g = ((val >> 5) & 0x3F) << 2;
                    let b = (val & 0x1F) << 3;
                    ((0xffu32 << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32
                }
                RlePixelFormat::Rgb15 => {
                    if src_pixel_idx * 2 + 1 >= decoded_data.len() { continue; }
                    let val = u16::from_le_bytes([
                        decoded_data[src_pixel_idx * 2],
                        decoded_data[src_pixel_idx * 2 + 1],
                    ]);
                    let r = ((val >> 10) & 0x1F) << 3;
                    let g = ((val >> 5) & 0x1F) << 3;
                    let b = (val & 0x1F) << 3;
                    ((0xffu32 << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32
                }
                RlePixelFormat::Rgb8 => {
                    if bpp == 32 {
                        if src_pixel_idx * 4 + 2 >= decoded_data.len() { continue; }
                        let b = decoded_data[src_pixel_idx * 4];
                        let g = decoded_data[src_pixel_idx * 4 + 1];
                        let r = decoded_data[src_pixel_idx * 4 + 2];
                        ((0xffu32 << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32) as i32
                    } else {
                        if src_pixel_idx >= decoded_data.len() { continue; }
                        let val = decoded_data[src_pixel_idx] as u32;
                        ((0xffu32 << 24) | (val << 16) | (val << 8) | val) as i32
                    }
                }
            };
            
            screen_pixels[dest_idx] = pixel_color;
        }
    }
}

fn read_der_length(der: &[u8], cursor: &mut usize) -> Option<usize> {
    if *cursor >= der.len() {
        return None;
    }
    let first = der[*cursor];
    *cursor += 1;
    
    if first < 0x80 {
        Some(first as usize)
    } else {
        let n_bytes = (first & 0x7F) as usize;
        if n_bytes == 0 || n_bytes > 4 || *cursor + n_bytes > der.len() {
            return None;
        }
        let mut len = 0;
        for _ in 0..n_bytes {
            len = (len << 8) | (der[*cursor] as usize);
            *cursor += 1;
        }
        Some(len)
    }
}

fn extract_raw_public_key(spki_der: &[u8]) -> Option<Vec<u8>> {
    let mut cursor = 0;
    
    // Read SEQUENCE tag
    if cursor >= spki_der.len() || spki_der[cursor] != 0x30 {
        return None;
    }
    cursor += 1;
    
    // Read SEQUENCE length
    let _seq_len = read_der_length(spki_der, &mut cursor)?;
    
    // Read algorithm (AlgorithmIdentifier SEQUENCE)
    if cursor >= spki_der.len() || spki_der[cursor] != 0x30 {
        return None;
    }
    cursor += 1;
    let alg_len = read_der_length(spki_der, &mut cursor)?;
    cursor += alg_len; // Skip AlgorithmIdentifier
    
    // Read subjectPublicKey BIT STRING (Tag 0x03)
    if cursor >= spki_der.len() || spki_der[cursor] != 0x03 {
        return None;
    }
    cursor += 1;
    
    let bit_str_len = read_der_length(spki_der, &mut cursor)?;
    if cursor + bit_str_len > spki_der.len() {
        return None;
    }
    
    // The BIT STRING value starts with a single byte indicating the number of unused bits (usually 0)
    let unused_bits = spki_der[cursor];
    if unused_bits != 0 {
        return None;
    }
    
    Some(spki_der[cursor + 1 .. cursor + bit_str_len].to_vec())
}


// ---------------------------------------------------------------------------
// Public session API (used by Android JNI and desktop Linux)
// ---------------------------------------------------------------------------

pub fn init_runtime() {
    #[cfg(feature = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Debug)
                .with_tag("RdpRustBackend")
        );
    }
    log::info!("Tokio Runtime loading");

    let mut rt_guard = RUNTIME.lock().unwrap();
    if rt_guard.is_none() {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            *rt_guard = Some(rt);
            log::info!("Tokio Runtime initialized successfully");
        }
    }
}

/// Start a new remote session and return its id for multi-tab clients.
/// Input APIs target the active session ([`set_active_session`]); new connects become active.
pub fn connect_session(
    host_str: String,
    port: i32,
    user_str: String,
    pass_str: String,
    domain_str: String,
    width: i32,
    height: i32,
    conn_mode_str: String,
    callback: SharedCallback,
) -> u64 {
    log::info!(
        "Connecting ({}). Host: {}, Port: {}, User: {}, Domain: {}, Width: {}, Height: {}",
        conn_mode_str, host_str, port, user_str, domain_str, width, height
    );

    let active = Arc::new(Mutex::new(true));

    if conn_mode_str == "VNC" {
        let (vnc_tx, mut vnc_rx) = mpsc::unbounded_channel::<vnc::X11Event>();
        let button_mask = Arc::new(Mutex::new(0u8));

        // Create session structure
        let session = Arc::new(Mutex::new(RdpSession {
            active: active.clone(),
            session_type: SessionType::Vnc {
                input_tx: vnc_tx,
                button_mask,
            },
            callback: callback.clone(),
        }));

        let session_id = register_session(session.clone());
        log::info!("Registered VNC session id={session_id}");

        let callback_clone = callback.clone();
        let active_clone = active.clone();
        
        let rt_guard = RUNTIME.lock().unwrap();
        if let Some(ref rt) = *rt_guard {
            rt.spawn(async move {
                let addr = format!("{}:{}", host_str, port);
                let status_msg = format!("Connecting VNC to {}...", addr);
                log::info!("{}", status_msg);
                notify_state_change(callback_clone.as_ref(), 1, &status_msg);

                match tokio::net::TcpStream::connect(&addr).await {
                    Ok(tcp_stream) => {
                        log::info!("TCP connected to VNC server. Starting RFB handshake...");
                        notify_state_change(callback_clone.as_ref(), 1, "RFB Handshake...");

                        let pass_clone = pass_str.clone();
                        let vnc_conn = vnc::VncConnector::new(tcp_stream)
                            .set_auth_method(async move { Ok(pass_clone) })
                            .add_encoding(vnc::VncEncoding::Zrle)
                            .add_encoding(vnc::VncEncoding::CopyRect)
                            .add_encoding(vnc::VncEncoding::Raw)
                            .add_encoding(vnc::VncEncoding::DesktopSizePseudo)
                            .allow_shared(true)
                            .set_pixel_format(vnc::PixelFormat::bgra());

                        match vnc_conn.build() {
                            Ok(builder) => {
                                match builder.try_start().await {
                                    Ok(started) => {
                                        match started.finish() {
                                            Ok(vnc_client) => {
                                                log::info!("VNC Connection Established!");
                                                notify_state_change(callback_clone.as_ref(), 2, "Connected (VNC)");

                                                let mut current_width = width;
                                                let mut current_height = height;
                                                let mut screen_pixels = vec![0i32; (current_width * current_height) as usize];
                                                
                                                 // Request initial full refresh
                                                 let _ = vnc_client.input(vnc::X11Event::FullRefresh).await;

                                                 // Loop for event polling and input processing
                                                 let mut needs_refresh = false;
                                                 let mut last_render = tokio::time::Instant::now();
                                                 
                                                 while *active_clone.lock().unwrap() {
                                                     let mut idle = true;

                                                     // 1. Process all pending incoming VNC events
                                                     loop {
                                                         match vnc_client.poll_event().await {
                                                             Ok(Some(vnc_event)) => {
                                                                 idle = false;
                                                                 match vnc_event {
                                                                     vnc::VncEvent::SetResolution(screen) => {
                                                                         log::info!("VNC resolution change: {}x{}", screen.width, screen.height);
                                                                         current_width = screen.width as i32;
                                                                         current_height = screen.height as i32;
                                                                         screen_pixels = vec![0i32; (current_width * current_height) as usize];
                                                                         notify_resolution_change(callback_clone.as_ref(), current_width, current_height);
                                                                     }
                                                                     vnc::VncEvent::RawImage(rect, data) => {
                                                                         copy_vnc_rect_to_screen(&mut screen_pixels, current_width, current_height, &rect, &data);
                                                                         needs_refresh = true;
                                                                     }
                                                                     vnc::VncEvent::Copy(dst, src) => {
                                                                         copy_vnc_screen_rect(&mut screen_pixels, current_width, current_height, &dst, &src);
                                                                         needs_refresh = true;
                                                                     }
                                                                     vnc::VncEvent::Error(err_msg) => {
                                                                         log::error!("VNC protocol error: {}", err_msg);
                                                                         notify_state_change(callback_clone.as_ref(), 3, &format!("VNC Error: {}", err_msg));
                                                                         return;
                                                                     }
                                                                     _ => {}
                                                                 }
                                                             }
                                                             Ok(None) => break,
                                                             Err(e) => {
                                                                 log::error!("VNC poll_event error: {:?}", e);
                                                                 notify_state_change(callback_clone.as_ref(), 3, &format!("Connection Error: {:?}", e));
                                                                 return;
                                                             }
                                                         }
                                                     }

                                                     // 2. Process all pending outgoing inputs
                                                     while let Ok(input_ev) = vnc_rx.try_recv() {
                                                         idle = false;
                                                         let _ = vnc_client.input(input_ev).await;
                                                         // Request a refresh immediately on input to ensure lowest interaction latency
                                                         let _ = vnc_client.input(vnc::X11Event::Refresh).await;
                                                     }

                                                     // 3. Render and request refresh periodically
                                                     if last_render.elapsed() >= Duration::from_millis(16) {
                                                         idle = false;
                                                         if needs_refresh {
                                                             push_frame(callback_clone.as_ref(), &screen_pixels, current_width, current_height);
                                                             needs_refresh = false;
                                                         }
                                                         // Unconditionally request refresh to ensure updates aren't stalled
                                                         let _ = vnc_client.input(vnc::X11Event::Refresh).await;
                                                         last_render = tokio::time::Instant::now();
                                                     }

                                                     // 4. Sleep a bit if there was no work to prevent 100% CPU hot loop
                                                     if idle {
                                                         tokio::time::sleep(Duration::from_millis(5)).await;
                                                     }
                                                 }
                                            }
                                            Err(e) => {
                                                let err_str = format!("VNC client finish failed: {:?}", e);
                                                log::error!("{}", err_str);
                                                notify_state_change(callback_clone.as_ref(), 3, &err_str);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let err_str = format!("VNC protocol start failed: {:?}", e);
                                        log::error!("{}", err_str);
                                        notify_state_change(callback_clone.as_ref(), 3, &err_str);
                                    }
                                }
                            }
                            Err(e) => {
                                let err_str = format!("VNC build failed: {:?}", e);
                                log::error!("{}", err_str);
                                notify_state_change(callback_clone.as_ref(), 3, &err_str);
                            }
                        }
                    }
                    Err(e) => {
                        let err_str = format!("Failed to connect TCP: {}", e);
                        log::error!("{}", err_str);
                        notify_state_change(callback_clone.as_ref(), 3, &err_str);
                    }
                }
            });
        }
        return session_id;
    }

    // --- RDP Mode Setup ---
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<FastPathInputEvent>();

    // Create session structure
    let session = Arc::new(Mutex::new(RdpSession {
        active: active.clone(),
        session_type: SessionType::Rdp { input_tx },
        callback: callback.clone(),
    }));

    let session_id = register_session(session.clone());
    log::info!("Registered RDP session id={session_id}");

        let callback_clone = callback.clone();
        let active_clone = active.clone();
        let screen_pixels_shared = Arc::new(Mutex::new(vec![0i32; (width * height) as usize]));

        // Run connection logic on tokio runtime
        let rt_guard = RUNTIME.lock().unwrap();
        if let Some(ref rt) = *rt_guard {
            rt.spawn(async move {
                struct AttemptConfig {
                    enable_credssp: bool,
                    domain: Option<String>,
                    desc: &'static str,
                }

                let mut attempts = Vec::new();
                if !domain_str.is_empty() {
                    attempts.push(AttemptConfig {
                        enable_credssp: true,
                        domain: Some(domain_str.clone()),
                        desc: "CredSSP (NLA) with domain",
                    });
                }
                attempts.push(AttemptConfig {
                    enable_credssp: true,
                    domain: None,
                    desc: "CredSSP (NLA) without domain",
                });
                attempts.push(AttemptConfig {
                    enable_credssp: false,
                    domain: None,
                    desc: "TLS Security (no NLA)",
                });

                let addr = format!("{}:{}", host_str, port);
                let mut last_error = String::new();
                let mut successful_conn = None;

                for (idx, attempt) in attempts.iter().enumerate() {
                    let status_msg = format!("Connecting... Attempt {}/{} ({})", idx + 1, attempts.len(), attempt.desc);
                    log::info!("{}", status_msg);
                    notify_state_change(callback_clone.as_ref(), 1, &status_msg);

                    match tokio::net::TcpStream::connect(&addr).await {
                        Ok(tcp_stream) => {
                            let local_addr = match tcp_stream.local_addr() {
                                Ok(addr) => addr,
                                Err(e) => {
                                    last_error = format!("Socket local_addr error: {}", e);
                                    continue;
                                }
                            };

                            let credentials = Credentials::UsernamePassword {
                                username: user_str.clone(),
                                password: pass_str.clone(),
                            };

                            let config = Config {
                                desktop_size: DesktopSize {
                                    width: width as u16,
                                    height: height as u16,
                                },
                                desktop_scale_factor: 100,
                                enable_tls: true,
                                enable_credssp: attempt.enable_credssp,
                                credentials,
                                domain: attempt.domain.clone(),
                                client_build: 2600,
                                client_name: "RustRDPVNC".to_string(),
                                keyboard_type: ironrdp_pdu::gcc::KeyboardType::IbmEnhanced,
                                keyboard_subtype: 0,
                                keyboard_functional_keys_count: 12,
                                keyboard_layout: 1033,
                                ime_file_name: String::new(),
                                bitmap: Some(BitmapConfig {
                                    lossy_compression: true,
                                    color_depth: 32,
                                    codecs: ironrdp_pdu::rdp::capability_sets::BitmapCodecs::default(),
                                }),
                                dig_product_id: String::new(),
                                client_dir: String::new(),
                                alternate_shell: String::new(),
                                work_dir: String::new(),
                                platform: ironrdp_pdu::rdp::capability_sets::MajorPlatformType::UNIX,
                                hardware_id: None,
                                request_data: None,
                                autologon: true,
                                enable_audio_playback: false,
                                performance_flags: ironrdp_pdu::rdp::client_info::PerformanceFlags::empty(),
                                license_cache: None,
                                timezone_info: ironrdp_pdu::rdp::client_info::TimezoneInfo::default(),
                                compression_type: None, // NO BULK COMPRESSION to avoid needing NCRUSH decompressors
                                enable_server_pointer: false,
                                pointer_software_rendering: false,
                                multitransport_flags: None,
                            };

                            let mut connector = ClientConnector::new(config, local_addr);

                            let mut drdynvc_client = DrdynvcClient::new();
                            let gfx_handler = MyGfxHandler {
                                callback: callback_clone.clone(),
                                screen_pixels: screen_pixels_shared.clone(),
                                width,
                                height,
                            };
                            let h264_decoder = ironrdp_egfx::decode::OpenH264Decoder::new().ok().map(|d| Box::new(d) as Box<dyn ironrdp_egfx::decode::H264Decoder>);
                            let gfx_client = GraphicsPipelineClient::new(Box::new(gfx_handler), h264_decoder);
                            drdynvc_client.attach_dynamic_channel(gfx_client);
                            connector.attach_static_channel(drdynvc_client);

                            notify_state_change(callback_clone.as_ref(), 1, &format!("Static channels count before connect_begin: {}", connector.static_channels.values().count()));
                            let mut framed = TokioFramed::new(tcp_stream);

                            let should_upgrade = match connect_begin(&mut framed, &mut connector).await {
                                Ok(su) => su,
                                Err(e) => {
                                    log::warn!("connect_begin failed for {}: {:?}", attempt.desc, e);
                                    last_error = format!("Handshake failed: {:?}", e);
                                    notify_state_change(callback_clone.as_ref(), 1, &format!("Attempt failed (connect_begin): {:?}", e));
                                    continue;
                                }
                            };

                            log::info!("Upgrading connection to TLS...");
                            let (tcp_stream, leftover) = framed.into_inner();

                            let tls_config = match rustls::ClientConfig::builder_with_provider(
                                Arc::new(rustls::crypto::ring::default_provider())
                            )
                            .with_safe_default_protocol_versions()
                            .unwrap()
                            .dangerous()
                            .with_custom_certificate_verifier(Arc::new(NoVerify))
                            .with_no_client_auth() {
                                config => config,
                            };

                            let server_name = match ServerName::try_from(host_str.clone()) {
                                Ok(sn) => sn.to_owned(),
                                Err(e) => {
                                    last_error = format!("Invalid host name: {:?}", e);
                                    continue;
                                }
                            };

                            let tls_connector = TlsConnector::from(Arc::new(tls_config));
                            let tls_stream = match tls_connector.connect(server_name, tcp_stream).await {
                                Ok(ts) => ts,
                                Err(e) => {
                                    log::warn!("TLS connect failed for {}: {:?}", attempt.desc, e);
                                    last_error = format!("TLS Connection Failed: {:?}", e);
                                    notify_state_change(callback_clone.as_ref(), 1, &format!("Attempt failed (TLS connect): {:?}", e));
                                    continue;
                                }
                            };

                            let (_, connection) = tls_stream.get_ref();
                            let certs = connection.peer_certificates().unwrap_or(&[]);
                            let server_public_key = if let Some(cert_der) = certs.first() {
                                match picky::x509::Cert::from_der(cert_der.as_ref()) {
                                    Ok(cert) => match cert.public_key().to_der() {
                                        Ok(spki_der) => {
                                            match extract_raw_public_key(&spki_der) {
                                                Some(raw_key) => raw_key,
                                                None => spki_der,
                                            }
                                        }
                                        Err(_) => Vec::new(),
                                    },
                                    Err(_) => Vec::new(),
                                 }
                            } else {
                                Vec::new()
                            };

                            let mut framed = TokioFramed::new_with_leftover(tls_stream, leftover);
                            let upgraded = mark_as_upgraded(should_upgrade, &mut connector);

                            let mut network_client = SimpleNetworkClient;
                            let rdp_server_name = RdpServerName::new(host_str.clone());
                            notify_state_change(callback_clone.as_ref(), 1, &format!("Static channels count before connect_finalize: {}", connector.static_channels.values().count()));

                            match connect_finalize(
                                upgraded,
                                connector,
                                &mut framed,
                                &mut network_client,
                                rdp_server_name,
                                server_public_key,
                                None,
                            )
                            .await {
                                Ok(res) => {
                                    log::info!("RDP connection finalized successfully using {}", attempt.desc);
                                    successful_conn = Some((framed, res));
                                    break;
                                }
                                Err(e) => {
                                    log::warn!("connect_finalize failed for {}: {:?}", attempt.desc, e);
                                    last_error = format!("Finalize failed: {:?}", e);
                                    notify_state_change(callback_clone.as_ref(), 1, &format!("Attempt failed (connect_finalize): {:?}", e));
                                    continue;
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("TCP Connect failed for {}: {}", attempt.desc, e);
                            last_error = format!("Network connection refused: {}", e);
                            notify_state_change(callback_clone.as_ref(), 1, &format!("Attempt failed (TCP connect): {}", e));
                            continue;
                        }
                    }
                }

                let (framed, res) = match successful_conn {
                    Some(f) => f,
                    None => {
                        log::error!("All RDP connection attempts failed. Last error: {}", last_error);
                        notify_state_change(callback_clone.as_ref(), 3, &format!("{}", last_error));
                        return;
                    }
                };

                notify_state_change(callback_clone.as_ref(), 2, "Connected successfully");

                let (mut reader, mut writer) = split_tokio_framed(framed);

                let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();

                let mut static_channels = res.static_channels;
                let drdynvc_channel_id = static_channels.get_channel_id_by_type::<DrdynvcClient>();

                // Spawning reader loop
                let callback_reader = callback_clone.clone();
                let active_reader = active_clone.clone();

                tokio::spawn(async move {
                    let mut screen_pixels = vec![0i32; (width * height) as usize];
                    let mut rdp6_decoder = BitmapStreamDecoder::default();
                    let mut decompressed_buf = Vec::new();

                    notify_state_change(callback_reader.as_ref(), 2, "[Rust Log] Reader loop started");
                    while *active_reader.lock().unwrap() {
                        match reader.read_pdu().await {
                            Ok((action, frame)) => {
                                if action == Action::FastPath {
                                    let mut cursor = ReadCursor::new(&frame);
                                    if let Ok(_fp_header) = FastPathHeader::decode(&mut cursor) {
                                        if let Ok(fp_update_pdu) = FastPathUpdatePdu::decode(&mut cursor) {
                                            if let Ok(fp_update) = FastPathUpdate::decode_with_code(fp_update_pdu.data, fp_update_pdu.update_code) {
                                                match fp_update {
                                                    FastPathUpdate::Bitmap(bitmap_data) => {
                                                        notify_state_change(callback_reader.as_ref(), 2, &format!("[Rust Log] FastPath Bitmap update: {} rects", bitmap_data.rectangles.len()));
                                                        for rect in bitmap_data.rectangles {
                                                            let w = rect.width as usize;
                                                            let h = rect.height as usize;
                                                            let compressed = rect.compression_flags.contains(Compression::BITMAP_COMPRESSION);

                                                            decompressed_buf.clear();
                                                            let format = if compressed {
                                                                if rect.bits_per_pixel == 32 {
                                                                    if rdp6_decoder.decode_bitmap_stream_to_rgb24(rect.bitmap_data, &mut decompressed_buf, w, h).is_ok() {
                                                                        Some(RlePixelFormat::Rgb24)
                                                                    } else {
                                                                        None
                                                                    }
                                                                } else {
                                                                    if let Ok(fmt) = ironrdp_graphics::rle::decompress(rect.bitmap_data, &mut decompressed_buf, w, h, rect.bits_per_pixel as usize) {
                                                                        Some(fmt)
                                                                    } else {
                                                                        None
                                                                    }
                                                                }
                                                            } else {
                                                                decompressed_buf.extend_from_slice(rect.bitmap_data);
                                                                if rect.bits_per_pixel == 32 {
                                                                    Some(RlePixelFormat::Rgb8)
                                                                } else if rect.bits_per_pixel == 24 {
                                                                    Some(RlePixelFormat::Rgb24)
                                                                } else if rect.bits_per_pixel == 16 {
                                                                    Some(RlePixelFormat::Rgb16)
                                                                } else {
                                                                    Some(RlePixelFormat::Rgb15)
                                                                }
                                                            };

                                                            if let Some(fmt) = format {
                                                                copy_bitmap_to_screen(
                                                                    &mut screen_pixels,
                                                                    width,
                                                                    height,
                                                                    &rect.rectangle,
                                                                    &decompressed_buf,
                                                                    rect.bits_per_pixel as usize,
                                                                    fmt
                                                                );
                                                            }
                                                        }
                                                        push_frame(callback_reader.as_ref(), &screen_pixels, width, height);
                                                    }
                                                    FastPathUpdate::SurfaceCommands(commands) => {
                                                        notify_state_change(callback_reader.as_ref(), 2, &format!("[Rust Log] FastPath SurfaceCommands: {} cmds", commands.len()));
                                                        for cmd in commands {
                                                            match cmd {
                                                                SurfaceCommand::SetSurfaceBits(bits) | SurfaceCommand::StreamSurfaceBits(bits) => {
                                                                    let dest = &bits.destination;
                                                                    let ext = &bits.extended_bitmap_data;
                                                                    let w = ext.width as usize;
                                                                    let h = ext.height as usize;

                                                                    decompressed_buf.clear();
                                                                    let format = if ext.codec_id == 9 || (ext.codec_id == 0 && ext.bpp == 32) {
                                                                        if rdp6_decoder.decode_bitmap_stream_to_rgb24(ext.data, &mut decompressed_buf, w, h).is_ok() {
                                                                            Some(RlePixelFormat::Rgb24)
                                                                        } else {
                                                                            None
                                                                        }
                                                                    } else if ext.codec_id == 0 {
                                                                        decompressed_buf.extend_from_slice(ext.data);
                                                                        if ext.bpp == 32 {
                                                                            Some(RlePixelFormat::Rgb8)
                                                                        } else if ext.bpp == 24 {
                                                                            Some(RlePixelFormat::Rgb24)
                                                                        } else if ext.bpp == 16 {
                                                                            Some(RlePixelFormat::Rgb16)
                                                                        } else {
                                                                            Some(RlePixelFormat::Rgb15)
                                                                        }
                                                                    } else {
                                                                        None
                                                                    };

                                                                    if let Some(fmt) = format {
                                                                        copy_surface_to_screen(
                                                                            &mut screen_pixels,
                                                                            width,
                                                                            height,
                                                                            dest,
                                                                            &decompressed_buf,
                                                                            ext.bpp as usize,
                                                                            fmt
                                                                        );
                                                                    }
                                                                }
                                                                _ => {}
                                                            }
                                                        }
                                                        push_frame(callback_reader.as_ref(), &screen_pixels, width, height);
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                } else if action == Action::X224 {
                                    match ironrdp_connector::legacy::decode_send_data_indication(&frame) {
                                        Ok(data_ctx) => {
                                            if Some(data_ctx.channel_id) == drdynvc_channel_id {
                                                if let Some(svc) = static_channels.get_by_channel_id_mut(data_ctx.channel_id) {
                                                    match svc.process(data_ctx.user_data) {
                                                        Ok(response_pdus) => {
                                                            if !response_pdus.is_empty() {
                                                                match ironrdp_svc::client_encode_svc_messages(
                                                                    response_pdus,
                                                                    data_ctx.channel_id,
                                                                    data_ctx.initiator_id,
                                                                ) {
                                                                    Ok(response_bytes) => {
                                                                        let _ = out_tx.send(response_bytes);
                                                                    }
                                                                    Err(e) => {
                                                                        notify_state_change(callback_reader.as_ref(), 2, &format!("[Rust Log] client_encode_svc_messages error: {:?}", e));
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        Err(e) => {
                                                            notify_state_change(callback_reader.as_ref(), 2, &format!("[Rust Log] svc.process (drdynvc) error: {:?}", e));
                                                        }
                                                    }
                                                } else {
                                                    notify_state_change(callback_reader.as_ref(), 2, "[Rust Log] svc for drdynvc channel not found");
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            notify_state_change(callback_reader.as_ref(), 2, &format!("[Rust Log] decode_send_data_indication error: {:?}", e));
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                notify_state_change(callback_reader.as_ref(), 2, &format!("[Rust Log] RDP read frame error: {:?}", e));
                                log::error!("RDP read frame error: {:?}", e);
                                break;
                            }
                        }
                    }
                    *active_reader.lock().unwrap() = false;
                    notify_state_change(callback_reader.as_ref(), 0, "Disconnected");
                });

                // Input sender loop
                let mut write_buf = WriteBuf::new();
                while *active_clone.lock().unwrap() {
                    tokio::select! {
                        Some(event) = input_rx.recv() => {
                            let fp_input = if let Ok(input) = FastPathInput::new(vec![event]) {
                                input
                            } else {
                                continue;
                            };

                            write_buf.clear();
                            if ironrdp_core::encode_buf(&fp_input, &mut write_buf).is_ok() {
                                let _ = writer.write_all(write_buf.filled()).await;
                            }
                        }
                        Some(out_frame) = out_rx.recv() => {
                            let _ = writer.write_all(&out_frame).await;
                        }
                        _ = sleep(Duration::from_millis(50)) => {}
                    }
                }
            });
        }

    session_id
}

/// Route subsequent mouse/keyboard input to the given session (desktop multi-tab).
pub fn set_active_session(id: u64) {
    ACTIVE_SESSION_ID.store(id, AtomicOrdering::SeqCst);
}

/// Disconnect a single session by id.
pub fn disconnect_session_id(id: u64) {
    if id == 0 {
        return;
    }
    log::info!("Disconnect session id={id}");
    let mut sessions = SESSIONS.lock().unwrap();
    if let Some(session) = sessions.remove(&id) {
        let sess = session.lock().unwrap();
        *sess.active.lock().unwrap() = false;
        notify_state_change(sess.callback.as_ref(), 0, "Disconnected");
    }
    if ACTIVE_SESSION_ID.load(AtomicOrdering::SeqCst) == id {
        let next = sessions.keys().next().copied().unwrap_or(0);
        ACTIVE_SESSION_ID.store(next, AtomicOrdering::SeqCst);
    }
}

/// Disconnect every active session (Android single-client / app exit).
pub fn disconnect_session() {
    log::info!("Disconnect all sessions");
    let mut sessions = SESSIONS.lock().unwrap();
    for (id, session) in sessions.drain() {
        log::info!("Disconnecting session id={id}");
        let sess = session.lock().unwrap();
        *sess.active.lock().unwrap() = false;
        notify_state_change(sess.callback.as_ref(), 0, "Disconnected");
    }
    ACTIVE_SESSION_ID.store(0, AtomicOrdering::SeqCst);
}

/// action: 0 = move, 1 = left down, 2 = left up, 3 = right down, 4 = right up
pub fn send_mouse_event(x: i32, y: i32, action: i32) {
    with_active_session(|sess| {
        match &sess.session_type {
            SessionType::Rdp { input_tx } => {
                let flags = match action {
                    0 => PointerFlags::MOVE,
                    1 => PointerFlags::DOWN | PointerFlags::LEFT_BUTTON,
                    2 => PointerFlags::LEFT_BUTTON,
                    3 => PointerFlags::DOWN | PointerFlags::RIGHT_BUTTON,
                    4 => PointerFlags::RIGHT_BUTTON,
                    _ => PointerFlags::MOVE,
                };
                let mouse_pdu = MousePdu {
                    flags,
                    number_of_wheel_rotation_units: 0,
                    x_position: x as u16,
                    y_position: y as u16,
                };
                let event = FastPathInputEvent::MouseEvent(mouse_pdu);
                let _ = input_tx.send(event);
            }
            SessionType::Vnc { input_tx, button_mask } => {
                let mut mask = button_mask.lock().unwrap();
                match action {
                    1 => *mask |= 1,
                    2 => *mask &= !1,
                    3 => *mask |= 4,
                    4 => *mask &= !4,
                    _ => {}
                }
                let mouse_event = vnc::ClientMouseEvent {
                    position_x: x as u16,
                    position_y: y as u16,
                    bottons: *mask,
                };
                let event = vnc::X11Event::PointerEvent(mouse_event);
                let _ = input_tx.send(event);
            }
        }
    });
}

pub fn send_mouse_wheel_event(x: i32, y: i32, units: i32) {
    if units == 0 {
        return;
    }
    with_active_session(|sess| {
        match &sess.session_type {
            SessionType::Rdp { input_tx } => {
                // Expand total delta into multiple 120-unit notches (Windows WHEEL_DELTA).
                // ironrdp only packs the low 8 bits of rotation per PDU, so multi-notch
                // is the reliable way to scroll faster.
                let mut remaining = units.unsigned_abs().max(1);
                let negative = units < 0;

                let move_pdu = MousePdu {
                    flags: PointerFlags::MOVE,
                    number_of_wheel_rotation_units: 0,
                    x_position: x as u16,
                    y_position: y as u16,
                };
                let _ = input_tx.send(FastPathInputEvent::MouseEvent(move_pdu));

                // Cap bursts so a single frame cannot flood the input channel.
                let mut emitted = 0u32;
                while remaining > 0 && emitted < 24 {
                    let chunk = remaining.min(120) as i16;
                    remaining -= chunk as u32;
                    emitted += 1;

                    // ironrdp encodes `n as u8` into the low byte and sets WHEEL_NEGATIVE
                    // when n < 0. MS-RDP expects that low byte to be the *absolute*
                    // rotation, so for scroll-down use n = abs - 256.
                    let wheel_units = if negative { chunk - 256 } else { chunk };

                    let mouse_pdu = MousePdu {
                        flags: PointerFlags::VERTICAL_WHEEL,
                        number_of_wheel_rotation_units: wheel_units,
                        x_position: x as u16,
                        y_position: y as u16,
                    };
                    let _ = input_tx.send(FastPathInputEvent::MouseEvent(mouse_pdu));
                }
            }
            SessionType::Vnc { input_tx, button_mask } => {
                // Positive units = scroll up (button 4). Negative = scroll down (button 5).
                let mask = button_mask.lock().unwrap();
                let wheel_bit = if units > 0 { 8 } else { 16 };

                let event_move = vnc::X11Event::PointerEvent(vnc::ClientMouseEvent {
                    position_x: x as u16,
                    position_y: y as u16,
                    bottons: *mask,
                });
                let _ = input_tx.send(event_move);

                // Map Windows-style units to VNC click steps (≈120 units per step).
                let steps = ((units.unsigned_abs() + 59) / 60).clamp(1, 24);
                for _ in 0..steps {
                    let event_wheel_press = vnc::X11Event::PointerEvent(vnc::ClientMouseEvent {
                        position_x: x as u16,
                        position_y: y as u16,
                        bottons: *mask | wheel_bit,
                    });
                    let _ = input_tx.send(event_wheel_press);

                    let event_wheel_release = vnc::X11Event::PointerEvent(vnc::ClientMouseEvent {
                        position_x: x as u16,
                        position_y: y as u16,
                        bottons: *mask,
                    });
                    let _ = input_tx.send(event_wheel_release);
                }
            }
        }
    });
}

pub fn send_key_event(keycode: i32, pressed: i32) {
    with_active_session(|sess| {
        let down = pressed != 0;
        match &sess.session_type {
            SessionType::Rdp { input_tx } => {
                let flags = if pressed == 0 {
                    KeyboardFlags::RELEASE
                } else {
                    KeyboardFlags::empty()
                };

                let input_event = if keycode == 8 {
                    FastPathInputEvent::KeyboardEvent(flags, 0x0E)
                } else if keycode == 13 {
                    FastPathInputEvent::KeyboardEvent(flags, 0x1C)
                } else {
                    FastPathInputEvent::UnicodeKeyboardEvent(flags, keycode as u16)
                };

                let _ = input_tx.send(input_event);
            }
            SessionType::Vnc { input_tx, .. } => {
                let keysym = if keycode == 8 {
                    0xff08
                } else if keycode == 13 {
                    0xff0d
                } else {
                    keycode as u32
                };
                let event = vnc::X11Event::KeyEvent(vnc::ClientKeyEvent {
                    keycode: keysym,
                    down,
                });
                let _ = input_tx.send(event);
            }
        }
    });
}

pub fn send_scancode_event(scancode: i32, is_extended: bool, pressed: i32) {
    with_active_session(|sess| {
        let down = pressed != 0;
        match &sess.session_type {
            SessionType::Rdp { input_tx } => {
                let mut flags = if pressed == 0 {
                    KeyboardFlags::RELEASE
                } else {
                    KeyboardFlags::empty()
                };
                if is_extended {
                    flags |= KeyboardFlags::EXTENDED;
                }
                let event = FastPathInputEvent::KeyboardEvent(flags, scancode as u8);
                let _ = input_tx.send(event);
            }
            SessionType::Vnc { input_tx, .. } => {
                if let Some(keysym) = scancode_to_keysym(scancode as u32, is_extended) {
                    let event = vnc::X11Event::KeyEvent(vnc::ClientKeyEvent {
                        keycode: keysym,
                        down,
                    });
                    let _ = input_tx.send(event);
                }
            }
        }
    });
}
