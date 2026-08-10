use crate::problem::Problem;

pub async fn calculate() -> Problem {
    Problem::not_implemented(
        "/api/v1/calculations",
        "The calculation engine is scaffolded but not implemented in pass 1.",
    )
}
