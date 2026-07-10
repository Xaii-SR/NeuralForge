use super::registry::WorkerProfile;
use serde::Serialize;

/// Sprint 5 capability matcher. Pure - no I/O - so the ranking rule is
/// fully testable: capability coverage dominates, reliability breaks ties.
///
/// score = coverage * 10 + reliability, where coverage is the fraction of
/// required capabilities the profile has (case-insensitive). The x10
/// weight makes any capability difference outweigh the whole [0,1]
/// reliability range: a less reliable worker that CAN do the job beats a
/// perfectly reliable one that can't. With no requirements, everyone has
/// coverage 1.0 and pure reliability ordering falls out.
#[derive(Serialize, Clone, Debug)]
pub struct WorkerMatch {
    pub profile: WorkerProfile,
    pub score: f64,
    /// How many of the required capabilities this profile covers.
    pub matched: usize,
    pub missing: Vec<String>,
}

fn coverage(profile: &WorkerProfile, required: &[String]) -> (usize, Vec<String>) {
    let have: Vec<String> = profile.capabilities.iter().map(|c| c.to_lowercase()).collect();
    let mut matched = 0;
    let mut missing = Vec::new();
    for req in required {
        if have.contains(&req.to_lowercase()) {
            matched += 1;
        } else {
            missing.push(req.clone());
        }
    }
    (matched, missing)
}

/// Ranks profiles best-first. Every profile is returned (with its missing
/// capabilities listed) so a caller can see WHY the top pick won and
/// whether even the top pick is a partial match - honest ranking, not a
/// silent filter.
pub fn rank(profiles: &[WorkerProfile], required_capabilities: &[String]) -> Vec<WorkerMatch> {
    let mut ranked: Vec<WorkerMatch> = profiles
        .iter()
        .map(|p| {
            let (matched, missing) = coverage(p, required_capabilities);
            let cov = if required_capabilities.is_empty() {
                1.0
            } else {
                matched as f64 / required_capabilities.len() as f64
            };
            WorkerMatch { score: cov * 10.0 + p.reliability_score, profile: p.clone(), matched, missing }
        })
        .collect();
    ranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}

/// The routing decision: best fully- or best-available partial match.
/// None only when there are no profiles at all.
pub fn best_match(profiles: &[WorkerProfile], required_capabilities: &[String]) -> Option<WorkerMatch> {
    rank(profiles, required_capabilities).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, caps: &[&str], reliability: f64) -> WorkerProfile {
        WorkerProfile {
            id: id.to_string(),
            name: id.to_string(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            reliability_score: reliability,
            tasks_completed: 0,
            tasks_failed: 0,
        }
    }

    /// Sprint 5 test 1: a task requiring "security" selects the profile
    /// that has it over one that doesn't - even when the non-matching
    /// profile is far more reliable.
    #[test]
    fn security_requirement_prefers_the_capable_profile() {
        let profiles = vec![
            profile("generalist", &["coding"], 1.0),
            profile("sec-worker", &["coding", "security"], 0.4),
        ];
        let best = best_match(&profiles, &["security".to_string()]).unwrap();
        assert_eq!(best.profile.id, "sec-worker");
        assert!(best.missing.is_empty());
    }

    /// Acceptance-gate phrasing: a "testing" requirement routes to the
    /// test-capable profile.
    #[test]
    fn testing_requirement_routes_to_test_capable_profile() {
        let profiles = vec![
            profile("coder", &["coding", "refactoring"], 0.95),
            profile("tester", &["testing"], 0.7),
        ];
        let best = best_match(&profiles, &["testing".to_string()]).unwrap();
        assert_eq!(best.profile.id, "tester");
    }

    #[test]
    fn reliability_breaks_ties_between_equally_capable_profiles() {
        let profiles = vec![
            profile("flaky", &["testing"], 0.3),
            profile("solid", &["testing"], 0.9),
        ];
        let ranked = rank(&profiles, &["testing".to_string()]);
        assert_eq!(ranked[0].profile.id, "solid");
        assert_eq!(ranked[1].profile.id, "flaky");
    }

    #[test]
    fn no_requirements_orders_by_pure_reliability() {
        let profiles = vec![
            profile("b", &["anything"], 0.5),
            profile("a", &["unrelated"], 0.99),
        ];
        let ranked = rank(&profiles, &[]);
        assert_eq!(ranked[0].profile.id, "a");
    }

    #[test]
    fn partial_matches_report_what_is_missing() {
        let profiles = vec![profile("half", &["coding"], 1.0)];
        let ranked = rank(&profiles, &["coding".to_string(), "security".to_string()]);
        assert_eq!(ranked[0].matched, 1);
        assert_eq!(ranked[0].missing, vec!["security"]);
    }

    #[test]
    fn capability_match_is_case_insensitive() {
        let profiles = vec![profile("w", &["Security"], 0.5)];
        let best = best_match(&profiles, &["security".to_string()]).unwrap();
        assert!(best.missing.is_empty());
    }

    #[test]
    fn best_match_is_none_only_with_no_profiles() {
        assert!(best_match(&[], &["x".to_string()]).is_none());
    }
}
