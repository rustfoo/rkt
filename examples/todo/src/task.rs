use rkt::serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(crate = "rkt::serde")]
pub struct Task {
    pub id: i64,
    pub description: String,
    pub completed: bool,
}

#[derive(Debug, FromForm)]
pub struct Todo {
    pub description: String,
}

impl Task {
    pub async fn all(pool: &SqlitePool) -> sqlx::Result<Vec<Task>> {
        sqlx::query_as::<_, Task>(
            "SELECT id, description, completed FROM tasks ORDER BY id DESC"
        )
        .fetch_all(pool)
        .await
    }

    pub async fn insert(todo: Todo, pool: &SqlitePool) -> sqlx::Result<()> {
        sqlx::query("INSERT INTO tasks (description, completed) VALUES (?, 0)")
            .bind(todo.description)
            .execute(pool)
            .await
            .map(|_| ())
    }

    pub async fn toggle_with_id(id: i64, pool: &SqlitePool) -> sqlx::Result<()> {
        sqlx::query("UPDATE tasks SET completed = NOT completed WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map(|_| ())
    }

    pub async fn delete_with_id(id: i64, pool: &SqlitePool) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map(|_| ())
    }

    #[cfg(test)]
    pub async fn delete_all(pool: &SqlitePool) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM tasks")
            .execute(pool)
            .await
            .map(|_| ())
    }
}
