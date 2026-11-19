use std::time::Instant;

use crate::database::Store;

pub(crate) struct Deadline<'a>(&'a mut Store);

impl<'a> Deadline<'a> {
    pub fn install(store: &'a mut Store, deadline: Instant) -> Result<Self, String> {
        const CHECK_INTERVAL: i32 = 2000;
        store
            .connection
            .progress_handler(CHECK_INTERVAL, Some(move || Instant::now() >= deadline))
            .map_err(crate::storage_error)?;
        Ok(Self(store))
    }

    pub fn store(&self) -> &Store {
        self.0
    }
}

impl Drop for Deadline<'_> {
    fn drop(&mut self) {
        let _ = self.0.connection.progress_handler(0, None::<fn() -> bool>);
    }
}

#[cfg(test)]
mod tests {
    use super::Deadline;

    #[test]
    fn expired_deadline_interrupts_queries_and_drop_removes_the_handler() {
        let root = tempfile::tempdir().unwrap();
        let mut store = crate::database::open(root.path().join("test-search.sqlite")).unwrap();
        let sql = "WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<10000) SELECT sum(n) FROM numbers";
        let deadline = Deadline::install(&mut store, std::time::Instant::now()).unwrap();
        let error = deadline
            .store()
            .connection
            .query_row(sql, [], |row| row.get::<_, i64>(0))
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::OperationInterrupted)
        );
        drop(deadline);
        let sum: i64 = store
            .connection
            .query_row(sql, [], |row| row.get(0))
            .unwrap();
        assert_eq!(sum, 50_005_000);
    }
}
