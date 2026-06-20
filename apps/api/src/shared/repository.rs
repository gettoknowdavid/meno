/// Bind cursor parameters to a sqlx `QueryBuilder`.
///
/// Call after the main WHERE clause.  Appends:
///   AND (`ts_col`, `id_col`) < ($N, $N+1) (for DESC)
///   AND (`ts_col`, `id_col`) > ($N, $N+1) (for ASC)
pub fn push_cursor_condition(
    qb: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    ts_col: &str,
    id_col: &str,
    cursor_ts: Option<time::OffsetDateTime>,
    cursor_id: Option<uuid::Uuid>,
    order: crate::shared::pagination::Order,
) {
    if let (Some(ts), Some(id)) = (cursor_ts, cursor_id) {
        let op = order.cursor_op();
        qb.push(format!(" AND ({ts_col}, {id_col}) {op} ("))
            .push_bind(ts)
            .push(", ")
            .push_bind(id)
            .push(")");
    }
}

/// Push ORDER BY + LIMIT to a `QueryBuilder`. Always call last.
pub fn push_order_and_limit(
    qb: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    ts_col: &str,
    id_col: &str,
    order: crate::shared::pagination::Order,
    limit_plus_one: i64,
) {
    let dir = order.sql();
    qb.push(format!(" ORDER BY {ts_col} {dir}, {id_col} {dir}"))
        .push(" LIMIT ")
        .push_bind(limit_plus_one);
}
