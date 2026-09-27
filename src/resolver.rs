use crate::lockfile::Lockfile;
use crate::package_instance::PackageInstance;

// npm と同じく、まず自分の node_modules を見て、無ければ親の node_modules へ、ルートまでたどる
pub(crate) fn resolve_peer<'a>(
    lockfile: &'a Lockfile,
    from: &PackageInstance,
    name: &str,
) -> Option<&'a PackageInstance> {
    let mut directory = Some(from.path.as_str());
    while let Some(current) = directory {
        let candidate = if current.is_empty() {
            format!("node_modules/{name}")
        } else {
            format!("{current}/node_modules/{name}")
        };
        if let Some(found) = lockfile.get(&candidate) {
            return Some(found);
        }
        directory = parent_directory(current);
    }
    None
}

fn parent_directory(path: &str) -> Option<&str> {
    if path.is_empty() {
        return None;
    }
    Some(
        path.rsplit_once("/node_modules/")
            .map_or("", |(parent, _)| parent),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const NESTED: &str = include_str!("../tests/fixtures/lockfiles/nested/package-lock.json");
    const SCOPED: &str = include_str!("../tests/fixtures/lockfiles/scoped/package-lock.json");

    fn provider(lockfile: &Lockfile, from: &str, name: &str) -> Option<String> {
        let from = lockfile.get(from).expect("テストデータにある配置");
        resolve_peer(lockfile, from, name).map(|found| format!("{} {found}", found.path))
    }

    #[test]
    fn resolves_to_the_nearest_package_up_the_tree() {
        let lockfile = Lockfile::parse(NESTED).expect("読める");
        assert_eq!(
            provider(&lockfile, "node_modules/plugin-a", "next").as_deref(),
            Some("node_modules/next next@15.3.0")
        );
        assert_eq!(
            provider(
                &lockfile,
                "node_modules/legacy-host/node_modules/plugin-old",
                "next"
            )
            .as_deref(),
            Some("node_modules/legacy-host/node_modules/next next@14.2.0")
        );
        assert_eq!(
            provider(
                &lockfile,
                "node_modules/legacy-host/node_modules/next",
                "react"
            )
            .as_deref(),
            Some("node_modules/react react@18.2.0")
        );
    }

    #[test]
    fn resolves_peers_of_scoped_packages() {
        let lockfile = Lockfile::parse(SCOPED).expect("読める");
        assert_eq!(
            provider(&lockfile, "node_modules/@acme/next-plugin", "next").as_deref(),
            Some("node_modules/next next@15.3.0")
        );
    }

    #[test]
    fn finds_nothing_when_the_peer_is_not_installed() {
        let lockfile = Lockfile::parse(NESTED).expect("読める");
        assert_eq!(provider(&lockfile, "node_modules/plugin-a", "vue"), None);
    }

    #[test]
    fn walks_up_through_scoped_and_nested_directories() {
        assert_eq!(
            parent_directory("node_modules/a/node_modules/@scope/b"),
            Some("node_modules/a")
        );
        assert_eq!(parent_directory("node_modules/@scope/b"), Some(""));
        assert_eq!(parent_directory("node_modules/a"), Some(""));
        assert_eq!(parent_directory(""), None);
    }
}
