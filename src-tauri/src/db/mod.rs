pub mod error;
pub mod models;
pub mod repository;
pub mod schema;

pub use error::DbError;
pub use models::{
    DownloadRecord, DownloadStatus, NewDownload, NewSegment, SegmentRecord, SegmentStatus,
};
pub use repository::Database;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_db_initialization() {
        let db = Database::open_in_memory().expect("failed to open in-memory db");
        let incomplete = db.get_incomplete_downloads().expect("failed to query");
        assert!(incomplete.is_empty());
    }

    #[test]
    fn test_download_crud_and_status() {
        let db = Database::open_in_memory().expect("db open");

        // Insert
        let new_dl = NewDownload::new(
            "https://example.com/file.zip",
            "file.zip",
            "/downloads/file.zip",
        )
        .with_total_size(1024 * 1024)
        .with_category("archives")
        .with_priority(5);

        let dl_id = db.insert_download(&new_dl).expect("insert download");
        assert!(dl_id > 0);

        // Get by ID
        let record = db
            .get_download(dl_id)
            .expect("get download")
            .expect("should exist");
        assert_eq!(record.id, dl_id);
        assert_eq!(record.url, "https://example.com/file.zip");
        assert_eq!(record.file_name, "file.zip");
        assert_eq!(record.save_path, "/downloads/file.zip");
        assert_eq!(record.total_size, Some(1024 * 1024));
        assert_eq!(record.downloaded_bytes, 0);
        assert_eq!(record.status, DownloadStatus::Pending);
        assert_eq!(record.category.as_deref(), Some("archives"));
        assert_eq!(record.priority, 5);
        assert!(record.completed_at.is_none());

        // Update progress
        db.update_download_progress(dl_id, 512 * 1024, None)
            .expect("update progress");
        let updated = db.get_download(dl_id).unwrap().unwrap();
        assert_eq!(updated.downloaded_bytes, 512 * 1024);

        // Update status to active
        db.update_download_status(dl_id, DownloadStatus::Active, None)
            .expect("status update");
        let active = db.get_download(dl_id).unwrap().unwrap();
        assert_eq!(active.status, DownloadStatus::Active);

        // Update status to completed
        db.update_download_status(dl_id, DownloadStatus::Completed, None)
            .expect("complete update");
        let completed = db.get_download(dl_id).unwrap().unwrap();
        assert_eq!(completed.status, DownloadStatus::Completed);
        assert!(completed.completed_at.is_some());

        // List downloads
        let all = db.list_downloads().expect("list");
        assert_eq!(all.len(), 1);

        // Incomplete list should now be empty since it is completed
        let incomplete = db.get_incomplete_downloads().expect("incomplete");
        assert!(incomplete.is_empty());
    }

    #[test]
    fn test_segments_and_cascade_delete() {
        let db = Database::open_in_memory().expect("db open");

        let dl_id = db
            .insert_download(&NewDownload::new(
                "https://example.com/test.iso",
                "test.iso",
                "/downloads/test.iso",
            ))
            .expect("insert");

        let segments = vec![
            NewSegment::new(0, 0, 999, Some("/tmp/part0".into())),
            NewSegment::new(1, 1000, 1999, Some("/tmp/part1".into())),
            NewSegment::new(2, 2000, 2999, Some("/tmp/part2".into())),
        ];

        db.insert_segments(dl_id, &segments)
            .expect("insert segments");

        let fetched_segs = db.get_segments(dl_id).expect("get segments");
        assert_eq!(fetched_segs.len(), 3);
        assert_eq!(fetched_segs[0].index_num, 0);
        assert_eq!(fetched_segs[0].start_byte, 0);
        assert_eq!(fetched_segs[0].end_byte, 999);
        assert_eq!(fetched_segs[0].remaining_bytes(), 1000);
        assert!(!fetched_segs[0].is_completed());

        // Update segment progress
        let seg0_id = fetched_segs[0].id;
        db.update_segment_progress(seg0_id, 1000, SegmentStatus::Completed)
            .expect("update segment");

        let updated_segs = db.get_segments(dl_id).unwrap();
        assert_eq!(updated_segs[0].downloaded, 1000);
        assert_eq!(updated_segs[0].status, SegmentStatus::Completed);
        assert_eq!(updated_segs[0].remaining_bytes(), 0);
        assert!(updated_segs[0].is_completed());

        // Verify foreign key CASCADE delete
        let deleted = db.delete_download(dl_id).expect("delete download");
        assert!(deleted);
        assert!(db.get_download(dl_id).unwrap().is_none());
        assert!(db.get_segments(dl_id).unwrap().is_empty());
    }

    #[test]
    fn test_reset_interrupted_tasks() {
        let db = Database::open_in_memory().expect("db open");

        let dl1 = db
            .insert_download(&NewDownload::new("https://example.com/1", "1", "/1"))
            .unwrap();
        let dl2 = db
            .insert_download(&NewDownload::new("https://example.com/2", "2", "/2"))
            .unwrap();

        db.update_download_status(dl1, DownloadStatus::Active, None)
            .unwrap();
        db.update_download_status(dl2, DownloadStatus::Pending, None)
            .unwrap();

        let affected = db.reset_interrupted_tasks().unwrap();
        assert_eq!(affected, 1);

        let dl1_after = db.get_download(dl1).unwrap().unwrap();
        assert_eq!(dl1_after.status, DownloadStatus::Paused);

        let dl2_after = db.get_download(dl2).unwrap().unwrap();
        assert_eq!(dl2_after.status, DownloadStatus::Pending);
    }

    #[test]
    fn test_settings_storage() {
        let db = Database::open_in_memory().expect("db open");

        assert_eq!(db.get_setting("max_connections").unwrap(), None);

        db.set_setting("max_connections", "8").unwrap();
        assert_eq!(
            db.get_setting("max_connections").unwrap(),
            Some("8".to_string())
        );

        // Update existing setting
        db.set_setting("max_connections", "16").unwrap();
        assert_eq!(
            db.get_setting("max_connections").unwrap(),
            Some("16".to_string())
        );
    }
}
