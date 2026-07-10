plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.rosace.demo_app"
    compileSdk = 34

    defaultConfig {
        applicationId = "dev.rosace.demo_app"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }
}

// Builds the Rust cdylib for the target ABI(s) and stages it into
// src/main/jniLibs/<abi>/ before Gradle's own resource-merge step picks it
// up via the jniLibs.srcDirs above — the Android counterpart to Step 2's
// Xcode PBXShellScriptBuildPhase. Verified: this task, followed by
// assembleDebug, produces a real .so-containing APK (see .steering/
// PHASE_24.md's Step 3 verification note); NDK path matches this machine's
// install and isn't yet configurable — a real per-project setup would read
// it from ANDROID_NDK_HOME, tracked as follow-up.
tasks.register("cargoBuildAndroid") {
    doLast {
        val abi = "arm64-v8a"
        val rustTriple = "aarch64-linux-android"
        // NDK root from the environment, not a hardcoded machine path —
        // ANDROID_NDK_HOME if set, else the newest version under
        // $ANDROID_HOME/ndk. Host-tag ("darwin-x86_64" etc.) still assumes
        // the NDK's own prebuilt-toolchain naming; only macOS/Linux/Windows
        // x86_64 hosts are handled, matching what this project has
        // actually been verified on (see .steering/CRATE_CONTRACTS.md
        // Known Issues) — ARM-host NDK layouts are a follow-up.
        val ndkHome = System.getenv("ANDROID_NDK_HOME")
            ?: File(System.getenv("ANDROID_HOME") ?: "${System.getProperty("user.home")}/Library/Android/sdk", "ndk")
                .listFiles()?.maxByOrNull { it.name }?.absolutePath
            ?: throw GradleException("Set ANDROID_NDK_HOME, or install an NDK under \$ANDROID_HOME/ndk")
        val hostTag = when {
            org.gradle.internal.os.OperatingSystem.current().isMacOsX -> "darwin-x86_64"
            org.gradle.internal.os.OperatingSystem.current().isLinux -> "linux-x86_64"
            else -> "windows-x86_64"
        }
        val minSdk = 24
        val linker = "$ndkHome/toolchains/llvm/prebuilt/$hostTag/bin/aarch64-linux-android$minSdk-clang"
        // Plain ProcessBuilder, not Gradle's exec DSL block — that's a
        // Project extension function not reliably reachable from inside a
        // registered task's doLast across Gradle/Kotlin-DSL versions
        // (confirmed: "Unresolved reference 'exec'" against this project's
        // Gradle 9.4 — plain JVM process APIs sidestep that entirely).
        val processBuilder = ProcessBuilder(
            "cargo", "build", "--lib", "--target", rustTriple, "--release"
        )
        processBuilder.directory(rootProject.projectDir.parentFile)
        processBuilder.environment()["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"] = linker
        processBuilder.inheritIO()
        val exitCode = processBuilder.start().waitFor()
        if (exitCode != 0) {
            throw GradleException("cargo build failed with exit code $exitCode")
        }
        val src = rootProject.projectDir.parentFile
            .resolve("target/$rustTriple/release/libdemo_app.so")
        val destDir = projectDir.resolve("src/main/jniLibs/$abi")
        destDir.mkdirs()
        src.copyTo(destDir.resolve("libdemo_app.so"), overwrite = true)
    }
}

tasks.named("preBuild") {
    dependsOn("cargoBuildAndroid")
}

dependencies {
}
