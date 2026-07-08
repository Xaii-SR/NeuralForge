/// Pure requirement validation - no DB, no Tauri, no LLM. This is the
/// first governance boundary: a weak request must be rejected here,
/// before a requirement row exists and long before any model call.
/// Returns every problem found, not just the first, so the frontend can
/// show a complete fix-list in one round trip.
pub struct RequirementInput<'a> {
    pub title: &'a str,
    pub intent: &'a str,
    pub acceptance_criteria: &'a [String],
}

const MIN_TITLE_CHARS: usize = 4;
const MIN_INTENT_CHARS: usize = 10;
const MIN_CRITERION_CHARS: usize = 5;
const MAX_TITLE_CHARS: usize = 200;

pub fn validate(input: &RequirementInput) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();

    let title = input.title.trim();
    if title.len() < MIN_TITLE_CHARS {
        problems.push(format!("title must be at least {MIN_TITLE_CHARS} characters"));
    }
    if title.len() > MAX_TITLE_CHARS {
        problems.push(format!("title must be at most {MAX_TITLE_CHARS} characters"));
    }

    let intent = input.intent.trim();
    if intent.len() < MIN_INTENT_CHARS {
        problems.push(format!("intent must be at least {MIN_INTENT_CHARS} characters - describe what should change and why"));
    }

    if input.acceptance_criteria.is_empty() {
        problems.push("at least one acceptance criterion is required".to_string());
    }
    for (i, criterion) in input.acceptance_criteria.iter().enumerate() {
        if criterion.trim().len() < MIN_CRITERION_CHARS {
            problems.push(format!("acceptance criterion {} is too short - state a checkable outcome", i + 1));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn criteria(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn valid_requirement_passes() {
        let ac = criteria(&["the function returns 4 for add(2, 2)"]);
        let input = RequirementInput {
            title: "Add doc comment to add()",
            intent: "Clarify what the addition function computes for future readers",
            acceptance_criteria: &ac,
        };
        assert!(validate(&input).is_ok());
    }

    #[test]
    fn empty_everything_reports_every_problem_not_just_the_first() {
        let ac: Vec<String> = vec![];
        let input = RequirementInput { title: "", intent: "", acceptance_criteria: &ac };
        let problems = validate(&input).unwrap_err();
        assert!(problems.len() >= 3, "expected title + intent + criteria problems, got: {problems:?}");
    }

    #[test]
    fn whitespace_only_fields_are_rejected_like_empty_ones() {
        let ac = criteria(&["   "]);
        let input = RequirementInput { title: "   ", intent: "          ", acceptance_criteria: &ac };
        let problems = validate(&input).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("title")));
        assert!(problems.iter().any(|p| p.contains("intent")));
        assert!(problems.iter().any(|p| p.contains("criterion 1")));
    }

    #[test]
    fn vague_one_word_intent_is_rejected() {
        let ac = criteria(&["tests still pass afterwards"]);
        let input = RequirementInput { title: "Fix stuff", intent: "fix it", acceptance_criteria: &ac };
        assert!(validate(&input).is_err());
    }

    #[test]
    fn one_bad_criterion_among_good_ones_is_named_by_index() {
        let ac = criteria(&["the output contains the user's name", "ok"]);
        let input = RequirementInput {
            title: "Improve greeting output",
            intent: "The greeting should address the user by name for a personal touch",
            acceptance_criteria: &ac,
        };
        let problems = validate(&input).unwrap_err();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("criterion 2"));
    }
}
