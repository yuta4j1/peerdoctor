use crate::blocker::Blocker;
use crate::package_instance::PackageInstance;
use crate::target_package::TargetPackage;

pub fn check(package_instance: PackageInstance, target_package: TargetPackage) -> Option<Blocker> {
    let requirement = package_instance
        .peer_requirements
        .iter()
        .find(|requirement| requirement.package_name == target_package.name)?
        .clone();

    if requirement.range.satisfies(target_package.version) {
        return None;
    }

    Some(Blocker {
        package: package_instance,
        requirement,
    })
}
