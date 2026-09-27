use crate::package_instance::PeerSpec;
use crate::registry::{Packument, PublishedVersion};
use crate::version::{Version, VersionRange};

#[derive(Clone)]
pub enum Resolution {
    Satisfiable(Candidates),
    DeadEnd { latest: Option<Version> },
}

#[derive(Clone)]
pub struct Candidates {
    pub(crate) range: String,
    pub(crate) minimum: Version,
    pub(crate) newest: Version,
    pub(crate) latest: Option<Version>,
    pub(crate) notes: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Acceptance {
    Accepts,
    NoPeerDeclared,
    Rejects,
    NotARange,
}

impl Acceptance {
    fn accepts(self) -> bool {
        matches!(self, Self::Accepts | Self::NoPeerDeclared)
    }
}

struct Entry<'a> {
    published: &'a PublishedVersion,
    acceptance: Acceptance,
}

pub(crate) fn resolve(packument: &Packument, peer: &str, target: &Version) -> Resolution {
    let include_prerelease = target.is_prerelease();
    let entries: Vec<Entry> = packument
        .versions()
        .iter()
        .filter(|published| include_prerelease || !published.version.is_prerelease())
        .map(|published| Entry {
            published,
            acceptance: acceptance(published, peer, target),
        })
        .collect();

    let mut notes = Vec::new();
    let mut intervals: Vec<&[Entry]> = Vec::new();
    let mut ends: Vec<Option<&Version>> = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        if !entries[index].acceptance.accepts() {
            index += 1;
            continue;
        }
        let start = index;
        while index < entries.len() && entries[index].acceptance.accepts() {
            index += 1;
        }
        let run = &entries[start..index];
        if run.iter().all(|entry| entry.published.deprecated.is_some()) {
            notes.push(format!(
                "deprecated only, not suggested: {}",
                versions_text(run.iter().map(|entry| &entry.published.version))
            ));
            continue;
        }
        intervals.push(run);
        ends.push(entries.get(index).map(|entry| &entry.published.version));
    }

    let suggested: Vec<&Entry> = intervals.iter().flat_map(|run| run.iter()).collect();
    let usable: Vec<&Version> = suggested
        .iter()
        .filter(|entry| entry.published.deprecated.is_none())
        .map(|entry| &entry.published.version)
        .collect();
    let (Some(minimum), Some(newest)) = (usable.first(), usable.last()) else {
        return Resolution::DeadEnd {
            latest: packument.latest().cloned(),
        };
    };

    let deprecated: Vec<&Version> = suggested
        .iter()
        .filter(|entry| entry.published.deprecated.is_some())
        .map(|entry| &entry.published.version)
        .collect();
    if !deprecated.is_empty() {
        notes.push(format!("deprecated: {}", versions_text(deprecated)));
    }
    let without_peer: Vec<&Version> = suggested
        .iter()
        .filter(|entry| entry.acceptance == Acceptance::NoPeerDeclared)
        .map(|entry| &entry.published.version)
        .collect();
    if !without_peer.is_empty() {
        notes.push(format!(
            "no peer on {peer} declared: {}",
            versions_text(without_peer)
        ));
    }
    let not_a_range: Vec<&Version> = entries
        .iter()
        .filter(|entry| entry.acceptance == Acceptance::NotARange)
        .map(|entry| &entry.published.version)
        .collect();
    if !not_a_range.is_empty() {
        notes.push(format!(
            "peer on {peer} is not a version range, so not suggested: {}",
            versions_text(not_a_range)
        ));
    }

    let range = interval_range(&intervals, &ends);
    let range = if selects_exactly(&range, &entries, &suggested) {
        range
    } else {
        versions_text(suggested.iter().map(|entry| &entry.published.version)).replace(", ", " || ")
    };

    Resolution::Satisfiable(Candidates {
        range,
        minimum: (*minimum).clone(),
        newest: (*newest).clone(),
        latest: packument.latest().cloned(),
        notes,
    })
}

fn acceptance(published: &PublishedVersion, peer: &str, target: &Version) -> Acceptance {
    let Some(requirement) = published
        .peer_requirements
        .iter()
        .find(|requirement| requirement.package_name == peer)
    else {
        return Acceptance::NoPeerDeclared;
    };
    match &requirement.spec {
        PeerSpec::Range(range) if range.satisfies(target) => Acceptance::Accepts,
        PeerSpec::Range(_) => Acceptance::Rejects,
        PeerSpec::NotARange(_) => Acceptance::NotARange,
    }
}

fn interval_range(intervals: &[&[Entry]], ends: &[Option<&Version>]) -> String {
    intervals
        .iter()
        .zip(ends)
        .map(|(run, end)| {
            let start = &run[0].published.version;
            match end {
                Some(end) => format!(">={start} <{end}"),
                None => format!(">={start}"),
            }
        })
        .collect::<Vec<_>>()
        .join(" || ")
}

// 出した範囲を npm にそのまま渡したとき、選ばれうる版が候補とぴったり一致するかを確かめる
fn selects_exactly(range: &str, entries: &[Entry], suggested: &[&Entry]) -> bool {
    let Ok(range) = VersionRange::parse(range) else {
        return false;
    };
    entries.iter().all(|entry| {
        let expected = suggested
            .iter()
            .any(|candidate| std::ptr::eq(*candidate, entry));
        range.satisfies(&entry.published.version) == expected
    })
}

fn versions_text<'a>(versions: impl IntoIterator<Item = &'a Version>) -> String {
    versions
        .into_iter()
        .map(|version| version.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::registry::fixture::FixtureRegistry;

    fn resolve_fixture(package: &str, target: &str) -> Resolution {
        let packument = FixtureRegistry::new()
            .packument(package)
            .expect("テストデータにある");
        resolve(
            &packument,
            "next",
            &Version::parse(target).expect("読めるバージョン"),
        )
    }

    fn describe(resolution: &Resolution) -> String {
        match resolution {
            Resolution::Satisfiable(candidates) => format!(
                "{} | minimum {} / newest {} / latest {} | {}",
                candidates.range,
                candidates.minimum,
                candidates.newest,
                candidates
                    .latest
                    .as_ref()
                    .map_or("-".to_string(), |latest| latest.to_string()),
                candidates.notes.join("; ")
            ),
            Resolution::DeadEnd { latest } => format!(
                "dead end (latest {})",
                latest
                    .as_ref()
                    .map_or("-".to_string(), |latest| latest.to_string())
            ),
        }
    }

    #[test]
    fn finds_a_bounded_range_and_notes_deprecated_versions() {
        assert_eq!(
            describe(&resolve_fixture("plugin-a", "16.3.0")),
            ">=3.0.0 <4.0.0 | minimum 3.0.0 / newest 3.2.1 / latest 4.1.0 | deprecated: 3.0.5"
        );
    }

    #[test]
    fn reports_a_dead_end_when_no_version_accepts() {
        assert_eq!(
            describe(&resolve_fixture("plugin-c", "16.3.0")),
            "dead end (latest 1.1.0)"
        );
    }

    #[test]
    fn joins_non_contiguous_runs() {
        assert_eq!(
            describe(&resolve_fixture("plugin-flaky", "16.3.0")),
            ">=1.0.0 <1.1.0 || >=1.2.0 <1.3.0 | minimum 1.0.0 / newest 1.2.0 / latest 1.3.0 | "
        );
    }

    #[test]
    fn leaves_the_range_open_and_notes_versions_without_the_peer() {
        assert_eq!(
            describe(&resolve_fixture("@acme/next-plugin", "16.3.0")),
            ">=2.0.0 | minimum 2.0.0 / newest 3.0.0 / latest 3.0.0 | no peer on next declared: 3.0.0"
        );
    }

    #[test]
    fn suggested_ranges_select_exactly_the_accepting_versions() {
        let registry = FixtureRegistry::new();
        for package in ["plugin-a", "plugin-c", "plugin-flaky", "@acme/next-plugin"] {
            let packument = registry.packument(package).expect("テストデータにある");
            for target in ["14.2.0", "15.3.0", "16.3.0", "17.0.0"] {
                let target = Version::parse(target).expect("読めるバージョン");
                let Resolution::Satisfiable(candidates) = resolve(&packument, "next", &target)
                else {
                    continue;
                };
                let range = VersionRange::parse(&candidates.range).expect("出した範囲は読める");
                for published in packument.versions() {
                    if published.version.is_prerelease() || published.deprecated.is_some() {
                        continue;
                    }
                    let accepts = acceptance(published, "next", &target).accepts();
                    assert_eq!(
                        range.satisfies(&published.version),
                        accepts,
                        "{package} {} を next@{target} のとき {} は選ぶべきか",
                        published.version,
                        candidates.range
                    );
                }
            }
        }
    }

    #[test]
    fn skips_runs_that_are_deprecated_only() {
        let packument = Packument::parse(
            "example",
            r#"{
                "dist-tags": { "latest": "2.0.0" },
                "versions": {
                    "1.0.0": { "peerDependencies": { "next": "^16" }, "deprecated": "broken" },
                    "1.1.0": { "peerDependencies": { "next": "^15" } },
                    "2.0.0": { "peerDependencies": { "next": "^16" } }
                }
            }"#,
        )
        .expect("読める");
        let resolution = resolve(
            &packument,
            "next",
            &Version::parse("16.3.0").expect("読める"),
        );
        assert_eq!(
            describe(&resolution),
            ">=2.0.0 | minimum 2.0.0 / newest 2.0.0 / latest 2.0.0 | deprecated only, not suggested: 1.0.0"
        );
    }

    #[test]
    fn does_not_suggest_versions_whose_peer_is_not_a_range() {
        let packument = Packument::parse(
            "example",
            r#"{
                "versions": {
                    "1.0.0": { "peerDependencies": { "next": "latest" } },
                    "2.0.0": { "peerDependencies": { "next": "^16" } }
                }
            }"#,
        )
        .expect("読める");
        let resolution = resolve(
            &packument,
            "next",
            &Version::parse("16.3.0").expect("読める"),
        );
        assert_eq!(
            describe(&resolution),
            ">=2.0.0 | minimum 2.0.0 / newest 2.0.0 / latest - | peer on next is not a version range, so not suggested: 1.0.0"
        );
    }

    #[test]
    fn lists_versions_when_a_range_would_not_select_them() {
        let packument = Packument::parse(
            "example",
            r#"{
                "versions": {
                    "1.0.0": { "peerDependencies": { "next": ">=17.0.0-rc.0" } },
                    "2.0.0-beta.1": { "peerDependencies": { "next": ">=17.0.0-rc.0" } },
                    "2.0.0": { "peerDependencies": { "next": "^16" } }
                }
            }"#,
        )
        .expect("読める");
        let resolution = resolve(
            &packument,
            "next",
            &Version::parse("17.0.0-rc.1").expect("読める"),
        );
        assert_eq!(
            describe(&resolution),
            "1.0.0 || 2.0.0-beta.1 | minimum 1.0.0 / newest 2.0.0-beta.1 / latest - | "
        );
    }
}
