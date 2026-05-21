use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct PaginationParams {
    pub page: i64,
    pub limit: i64,
}
impl PaginationParams {
    pub fn new(page: i64, limit: i64) -> Self {
        Self { page, limit }
    }
    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.limit
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PaginationResponse<T> {
    pub total_pages: i64,
    pub current_page: i64,
    pub total_items: i64,
    pub data: Vec<T>,
}
