//! Emit the Foundations facts consumed by `foundations_content_audit.py`.

fn main() -> std::process::ExitCode {
    cadus_core::content_audit_facts::command()
}
