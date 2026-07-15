use crate::cli::DocsTopic;

pub fn render(topic: DocsTopic) -> &'static str {
    match topic {
        DocsTopic::Agent => include_str!("../docs/agent-usage.md"),
    }
}
