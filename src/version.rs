pub struct Version(String);

#[derive(Clone)]
pub struct VersionRange(String);

impl VersionRange {
    pub(crate) fn satisfies(&self, version: Version) -> bool {
        todo!("npm 互換の範囲判定。semver クレートを選定してから実装する")
    }
}
