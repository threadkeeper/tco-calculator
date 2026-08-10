use crate::problem::Problem;

pub async fn unauthorized() -> Problem {
    Problem::unauthorized(
        "/api/v1/projects",
        "Sign in with Microsoft to access saved projects.",
    )
}
