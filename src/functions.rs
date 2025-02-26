use crate::book::chapter::ChapterNumber;



#[derive(Debug)]
pub enum ChapterStatus {
    NotStarted,
    InProgress,
    Completed,
}

pub struct ChapterProgress {
    pub section_number: ChapterNumber,
    pub status: ChapterStatus,
    pub description: String,
}

pub struct StudyProgressResponse {
    pub plan: String,
    pub overall_progress: String,
    pub chapter_progress: Option<String>,
}

pub struct UpdateStudyProgressRequest {
    pub student_id: i64,
    pub book_id: i64,
    pub chapter_index: String,
    pub status: i8,
    pub description: String,
}

pub struct UpdateStudyPlanRequest {
    pub student_id: i64,
    pub book_id: i64,
    pub plan: String,
}

