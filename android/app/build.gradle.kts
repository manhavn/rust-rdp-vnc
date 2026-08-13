plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.rustai.rdp"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.rustai.rdp"
        minSdk = 26
        targetSdk = 35
        versionCode = 5
        versionName = "1.0.6"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        vectorDrawables {
            useSupportLibrary = true
        }
    }

    signingConfigs {
        val releaseKeystore = file("release.keystore")
        if (releaseKeystore.exists()) {
            create("release") {
                storeFile = releaseKeystore
                storePassword = "password"
                keyAlias = "release-key"
                keyPassword = "password"
                enableV1Signing = true
                enableV2Signing = true
            }
        }
    }

    buildTypes {
        release {
            val releaseConfig = signingConfigs.findByName("release")
            if (releaseConfig != null) {
                signingConfig = releaseConfig
            } else {
                // Fall back to debug signing if release key is missing
                signingConfig = signingConfigs.getByName("debug")
            }
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        compose = true
    }
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
    lint {
        checkReleaseBuilds = false
        abortOnError = false
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.2")
    implementation("androidx.activity:activity-compose:1.9.0")
    
    implementation(platform("androidx.compose:compose-bom:2024.06.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")

    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.5")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.1")
    androidTestImplementation(platform("androidx.compose:compose-bom:2024.06.00"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}

tasks.register<Exec>("buildRust") {
    val isRelease = !project.hasProperty("rustDebug")
    val targetDirName = if (isRelease) "release" else "debug"
    
    environment("ANDROID_NDK_HOME", "/home/dev/Android/android-sdk/ndk/25.2.9519653")
    workingDir("../../rust_rdp")
    
    val argsList = mutableListOf(
        "cargo", "ndk",
        "-t", "aarch64-linux-android",
        "-t", "x86_64-linux-android",
        "build",
        "--features", "android"
    )
    if (isRelease) {
        argsList.add("--release")
    }
    commandLine(argsList)
    
    doLast {
        val jniLibsDir = project.file("src/main/jniLibs")
        jniLibsDir.mkdirs()
        
        val ndkHome = "/home/dev/Android/android-sdk/ndk/25.2.9519653"
        
        val arm64So = project.file("../../target/aarch64-linux-android/$targetDirName/librust_rdp.so")
        val x86_64So = project.file("../../target/x86_64-linux-android/$targetDirName/librust_rdp.so")
        
        if (!arm64So.exists()) {
            throw GradleException("Built arm64 librust_rdp.so not found at ${arm64So.absolutePath}")
        }
        if (!x86_64So.exists()) {
            throw GradleException("Built x86_64 librust_rdp.so not found at ${x86_64So.absolutePath}")
        }

        project.copy {
            from(arm64So)
            into(project.file("src/main/jniLibs/arm64-v8a"))
        }
        project.copy {
            from("$ndkHome/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so")
            into(project.file("src/main/jniLibs/arm64-v8a"))
        }
        
        project.copy {
            from(x86_64So)
            into(project.file("src/main/jniLibs/x86_64"))
        }
        project.copy {
            from("$ndkHome/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/x86_64-linux-android/libc++_shared.so")
            into(project.file("src/main/jniLibs/x86_64"))
        }
    }
}

tasks.configureEach {
    if (name.startsWith("compileDebugKotlin") || name.startsWith("compileReleaseKotlin") ||
        name.startsWith("mergeDebugNativeLibs") || name.startsWith("mergeReleaseNativeLibs")) {
        dependsOn("buildRust")
    }
}
