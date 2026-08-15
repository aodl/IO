use std::fs;
use std::path::Path;

const REQUIRED_WORKFLOWS: &[&str] = &[
    ".github/workflows/test.yml",
    ".github/workflows/security.yml",
    ".github/workflows/reproducible-build.yml",
];

const EXACT_EVENT_SHA: &str =
    "${{ github.event_name == 'pull_request' && github.event.pull_request.head.sha || github.sha }}";

pub(crate) fn check_required_workflows_at(root: &Path) -> Result<(), String> {
    for path in REQUIRED_WORKFLOWS {
        let text =
            fs::read_to_string(root.join(path)).map_err(|err| format!("read {path}: {err}"))?;
        for required in [
            &format!("ref: {EXACT_EVENT_SHA}"),
            &format!("EXPECTED_SOURCE_SHA: {EXACT_EVENT_SHA}"),
            "actual_head=\"$(git rev-parse HEAD)\"",
            "validated source SHA: ${actual_head}",
            "${actual_head}\" != \"${EXPECTED_SOURCE_SHA}",
            "git merge-base --is-ancestor \"${PR_BASE_SHA}\" HEAD",
        ] {
            if !text.contains(required) {
                return Err(format!(
                    "{path}: missing exact-source guardrail `{required}`"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_workflows_validate_the_same_exact_event_sha() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        check_required_workflows_at(&root).unwrap();
    }
}
