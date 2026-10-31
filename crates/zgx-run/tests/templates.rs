//! `zgx new`'s templates audit clean (`zgx audit`), with and without
//! `--i18n`, through what they open: the check their CI workflow runs.
use zgx_run::audit::audit_app;
use zgx_script::scaffold::{TEMPLATES, create};

#[test]
fn every_template_audits_clean() {
    let root = std::env::temp_dir().join(format!("zgx-run-templates-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (template, _) in TEMPLATES {
        for i18n in [false, true] {
            let app = root.join(format!(
                "{template}-{}",
                if i18n { "i18n" } else { "plain" }
            ));
            create(&app, template, i18n).unwrap();
            let audit = audit_app(&app).unwrap_or_else(|e| panic!("{template}: {e}"));
            assert!(audit.findings.is_empty(), "{template}: {}", audit.to_json());
            assert!(audit.errors.is_empty(), "{template}: {}", audit.to_json());
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
