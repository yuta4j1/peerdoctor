use peerdoctor::{PackageInstance, PeerRequirement, TargetPackage, Version, VersionRange, check};

fn main() {
    // ロックファイルを読む（step 2）までの、手で用意した入力
    let target = TargetPackage::new("next", version("16.3.0"));
    let packages = [
        PackageInstance::new(
            "node_modules/plugin-a",
            "plugin-a",
            version("2.1.0"),
            vec![PeerRequirement::new("next", range("^14 || ^15"), false)],
        ),
        PackageInstance::new(
            "node_modules/plugin-b",
            "plugin-b",
            version("3.0.0"),
            vec![PeerRequirement::new("next", range(">=15 <17"), false)],
        ),
    ];

    println!("Target: {target}");
    for package in &packages {
        match check(package, &target) {
            Some(blocker) => println!("BLOCKER  {blocker}"),
            None => println!("OK       {package}"),
        }
    }
}

fn version(text: &str) -> Version {
    Version::parse(text).expect("手で用意したバージョンは読める")
}

fn range(text: &str) -> VersionRange {
    VersionRange::parse(text).expect("手で用意した範囲は読める")
}
