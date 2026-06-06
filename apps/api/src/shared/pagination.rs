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
impl<T> PaginationResponse<T> {
    pub fn build(limit: i64, page: i64, total: i64, data: Vec<T>) -> Self {
        let total_pages = if total == 0 {
            0
        } else {
            (total + limit - 1) / limit
        };
        Self {
            total_items: total,
            total_pages,
            current_page: page,
            data,
        }
    }
    pub fn empty(limit: i64, page: i64) -> Self {
        Self::build(limit, page, 0, Vec::<T>::new())
    }
}

pub enum PaginationDirection {
    Next,
    Previous,
}
