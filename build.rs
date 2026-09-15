use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    for (key, command, args) in [
        ("GIT_HASH", "git", vec!["rev-parse", "--short", "HEAD"]),
        ("RUSTC_VERSION", "rustc", vec!["--version"]),
    ] {
        let value = std::process::Command::new(command)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|| "unknown".into());
        println!("cargo:rustc-env={key}={value}");
    }
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=FFRS_SDK");
    let sdk = env::var_os("FFRS_SDK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory missing"))
            .join("5.6.1-0+UE5-FFRS")
            .join("CppSDK")
        });
    assert!(
        sdk.join("SDK/Basic.hpp").is_file(),
        "Set FFRS_SDK to the dumped CppSDK directory"
    );
    let generated = sdk_wrappers(
        &sdk,
        "Engine",
        &[
            "class UObject* UGameplayStatics::SpawnObject(",
            "class FName UKismetStringLibrary::Conv_StringToName(",
            "void UKismetRenderingLibrary::BeginDrawCanvasToRenderTarget(",
            "void UKismetRenderingLibrary::EndDrawCanvasToRenderTarget(",
            "class UTextureRenderTarget2D* UKismetRenderingLibrary::CreateRenderTarget2D(",
            "void UCanvas::K2_DrawTexture(",
        ],
    );
    let mut build = cxx_build::bridge("src/bridge.rs");
    build
        .files([
            "cpp/utils.cpp",
            "cpp/layout.cpp",
            "cpp/hud.cpp",
            "cpp/map.cpp",
            "cpp/encounter.cpp",
            "cpp/movies.cpp",
            "cpp/post_processing.cpp",
        ])
        .file(sdk.join("SDK/Basic.cpp"))
        .file(sdk.join("SDK/CoreUObject_functions.cpp"))
        .file(sdk.join("SDK/UMG_functions.cpp"))
        .file(generated)
        .include(".")
        .include(&sdk)
        .include(sdk.join("SDK"))
        .flag("/FIcpp/sdk_compat.hpp")
        .flag("/EHsc")
        .std("c++20")
        .flag_if_supported("/bigobj")
        .flag_if_supported("/utf-8")
        .warnings(false)
        .compile("ffrs_sdk_bridge");
    println!("cargo:rerun-if-changed=src/bridge.rs");
    println!("cargo:rerun-if-changed=cpp");
    println!("cargo:rerun-if-changed={}", sdk.display());
}

// Compile only the generated wrappers used by this mod. The external dump is unchanged.
fn sdk_wrappers(sdk: &Path, package: &str, signatures: &[&str]) -> PathBuf {
    let source = fs::read_to_string(sdk.join(format!("SDK/{package}_functions.cpp"))).unwrap();
    let mut wrappers = String::new();
    for signature in signatures {
        let start = source
            .find(signature)
            .unwrap_or_else(|| panic!("Missing SDK wrapper: {signature}"));
        let body = &source[start..];
        let end = body.find("\n}").expect("SDK wrapper closing brace missing") + 2;
        wrappers.push_str(&body[..end]);
        wrappers.push('\n');
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join(format!("sdk_{package}.cpp"));
    fs::write(&output, format!("#include \"{package}_classes.hpp\"\n#include \"{package}_parameters.hpp\"\nSDK_NAMESPACE_START\n{wrappers}\nSDK_NAMESPACE_END\n")).unwrap();
    output
}
