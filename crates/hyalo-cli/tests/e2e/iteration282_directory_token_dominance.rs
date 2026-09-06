//! Directory overlap has its own majority-prefix admission (DEC-329).
use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

const OLD: &str = "/admin/identity-and-access-management/using-saml-for-enterprise-iam/saml-configuration-reference";
const NEW: &str = "admin/managing-iam/iam-configuration-reference/saml-configuration-reference.md";

fn fixture(body: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join(NEW);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, "# SAML\n\n## Attributes\n").unwrap();
    std::fs::write(tmp.path().join("src.md"), body).unwrap();
    tmp
}

fn command(tmp: &TempDir, indexed: bool) -> Command {
    let mut cmd = crate::common::hyalo_no_hints();
    cmd.arg("--dir").arg(tmp.path()).args(["--site-prefix", ""]);
    if indexed {
        cmd.arg("--index-file").arg(tmp.path().join(".hyalo-index"));
    }
    cmd
}

fn json(tmp: &TempDir, indexed: bool, args: &[&str]) -> Value {
    let output = command(tmp, indexed)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn minimal_saml_fixture_restores_point_804_on_disk_and_index() {
    for indexed in [false, true] {
        let original = format!("[AUTOTITLE]({OLD})\n");
        let tmp = fixture(&original);
        if indexed {
            json(&tmp, false, &["create-index"]);
        }
        let plan = json(&tmp, indexed, &["links", "fix", "--dry-run"]);
        let fixes = plan["results"]["fuzzy_fixes"].as_array().unwrap();
        assert_eq!(fixes.len(), 1, "{plan}");
        assert_eq!(fixes[0]["new_target"], NEW);
        assert_eq!(
            format!("{:.3}", fixes[0]["confidence"].as_f64().unwrap()),
            "0.804"
        );
        assert_eq!(fixes[0]["below_floor"], false);
        assert_eq!(plan["results"]["fuzzy_below_floor"], 0);
        command(&tmp, indexed)
            .args(["links", "fix", "--dry-run", "--format", "text"])
            .assert()
            .success()
            .stdout(predicates::str::contains("0.80"));
        // Immediately above the recovered confidence it is still withheld.
        let high_floor = json(
            &tmp,
            indexed,
            &["links", "fix", "--apply", "--min-confidence", "0.805"],
        );
        assert_eq!(high_floor["results"]["fuzzy_fixes"][0]["below_floor"], true);
        json(&tmp, indexed, &["links", "fix", "--apply"]);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("src.md")).unwrap(),
            original
        );
        let applied = json(&tmp, indexed, &["links", "fix", "--apply", "--apply-fuzzy"]);
        assert_eq!(applied["results"]["fuzzy"], 1, "{applied}");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("src.md")).unwrap(),
            format!("[AUTOTITLE](/{})\n", NEW.trim_end_matches(".md"))
        );
        let after = json(&tmp, indexed, &["links", "fix", "--dry-run"]);
        assert_eq!(after["results"]["broken"], 0, "{after}");
    }
}

#[test]
fn renamed_directory_rewrites_supported_link_forms_and_preserves_protected_bytes() {
    for indexed in [false, true] {
        let links = format!("[inline]({OLD}#attributes \"title\")\n[[{OLD}#Attributes|label]]\n");
        // Reference links are currently outside links fix's inventory.
        let protected = format!(
            "\n[reference][saml]\n\n[saml]: {OLD}#attributes \"title\"\n\n`[code]({OLD})`\n\n```md\n[code]({OLD})\n```\n"
        );
        let tmp = fixture(&format!("{links}{protected}"));
        if indexed {
            json(&tmp, false, &["create-index"]);
        }
        let applied = json(&tmp, indexed, &["links", "fix", "--apply", "--apply-fuzzy"]);
        assert_eq!(applied["results"]["failed"], 0, "{applied}");
        let new = NEW.trim_end_matches(".md");
        let expected =
            format!("[inline](/{new}#attributes \"title\")\n[[{new}#Attributes|label]]\n");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("src.md")).unwrap(),
            format!("{expected}{protected}")
        );
        let after = json(&tmp, indexed, &["links", "fix", "--dry-run"]);
        assert_eq!(after["results"]["broken"], 0, "{after}");
        assert_eq!(after["results"]["broken_anchors"], 0, "{after}");
    }
}
