plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.roborazzi)
}

// Builds are signed with a personal keystore (WAYBILL_ANDROID_KEYSTORE, by
// default ~/.android/waybill-release.jks) when
// WAYBILL_ANDROID_KEYSTORE_PASSWORD is set, so one install can be upgraded
// by the next. Without it the SDK's debug key signs.
val keystore = file(
    System.getenv("WAYBILL_ANDROID_KEYSTORE")
        ?: "${System.getProperty("user.home")}/.android/waybill-release.jks",
)
val keystorePassword = System.getenv("WAYBILL_ANDROID_KEYSTORE_PASSWORD")

android {
    namespace = "io.github.nathanielinman.waybill"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.nathanielinman.waybill"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    signingConfigs {
        if (keystore.exists() && keystorePassword != null) {
            create("personal") {
                storeFile = keystore
                storePassword = keystorePassword
                keyAlias = "waybill"
                keyPassword = keystorePassword
            }
        }
    }

    buildTypes {
        val personal = signingConfigs.findByName("personal")
        debug {
            if (personal != null) signingConfig = personal
        }
        release {
            isMinifyEnabled = false
            signingConfig = personal ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    testOptions {
        unitTests.isIncludeAndroidResources = true
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

// The screenshots in the README: `./gradlew recordRoborazziDebug` draws
// every view from the demo list the Rust tests write.
roborazzi {
    outputDir.set(rootProject.file("../docs/assets/screens/android"))
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.work.runtime.ktx)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.serialization.json)
    debugImplementation(libs.androidx.compose.ui.tooling)
    debugImplementation(libs.androidx.compose.ui.test.manifest)
    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
    testImplementation(libs.roborazzi)
    testImplementation(libs.roborazzi.compose)
    testImplementation(platform(libs.androidx.compose.bom))
    testImplementation(libs.androidx.compose.ui.test.junit4)
}
