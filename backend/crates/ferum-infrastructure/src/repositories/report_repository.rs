use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{posts, reports, sea_orm_active_enums, threads};
use ferum_application::shared::AppError;
use ferum_domain::models::report::{Report, ReportStatus};
use ferum_domain::repositories::report_repository::{ReportRepository, ReportStatusCounts};

/// Restricts `query` to reports whose target (directly via `thread_id`, or
/// indirectly via `post_id` -> the post's thread) belongs to one of `category_ids`.
/// Built as two `IN (subquery)` conditions OR'd together since `reports` has no
/// direct FK to `categories` — it links through either `threads` or `posts`.
fn scope_to_categories(
    query: sea_orm::Select<reports::Entity>,
    category_ids: &[Uuid],
) -> sea_orm::Select<reports::Entity> {
    let thread_ids_in_categories = sea_orm::sea_query::Query::select()
        .column(threads::Column::Id)
        .from(threads::Entity)
        .and_where(threads::Column::CategoryId.is_in(category_ids.to_vec()))
        .to_owned();

    let post_ids_in_categories = sea_orm::sea_query::Query::select()
        .column(posts::Column::Id)
        .from(posts::Entity)
        .and_where(
            posts::Column::ThreadId.in_subquery(
                sea_orm::sea_query::Query::select()
                    .column(threads::Column::Id)
                    .from(threads::Entity)
                    .and_where(threads::Column::CategoryId.is_in(category_ids.to_vec()))
                    .to_owned(),
            ),
        )
        .to_owned();

    query.filter(
        Condition::any()
            .add(reports::Column::ThreadId.in_subquery(thread_ids_in_categories))
            .add(reports::Column::PostId.in_subquery(post_ids_in_categories)),
    )
}

pub struct PgReportRepository {
    db: DatabaseConnection,
}

impl PgReportRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn entity_to_domain(m: reports::Model) -> Report {
    Report {
        id: m.id,
        reporter_id: m.reporter_id,
        post_id: m.post_id,
        thread_id: m.thread_id,
        reason: m.reason,
        status: match m.status {
            sea_orm_active_enums::ReportStatus::Pending => ReportStatus::Pending,
            sea_orm_active_enums::ReportStatus::Resolved => ReportStatus::Resolved,
            sea_orm_active_enums::ReportStatus::Dismissed => ReportStatus::Dismissed,
        },
        moderator_notes: m.moderator_notes,
        resolved_by_id: m.resolved_by_id,
        resolved_at: m.resolved_at.map(|t| t.with_timezone(&Utc)),
        target_deleted_at: m.target_deleted_at.map(|t| t.with_timezone(&Utc)),
        created_at: m.created_at.with_timezone(&Utc),
    }
}

#[async_trait]
impl ReportRepository for PgReportRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Report>, AppError> {
        Ok(reports::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(entity_to_domain))
    }

    async fn count_by_status<'a>(
        &self,
        category_ids: Option<&'a [Uuid]>,
    ) -> Result<ReportStatusCounts, AppError> {
        let base = |status: sea_orm_active_enums::ReportStatus| {
            let mut q = reports::Entity::find().filter(reports::Column::Status.eq(status));
            if let Some(cats) = category_ids {
                q = scope_to_categories(q, cats);
            }
            q
        };
        let (pending, resolved, dismissed) = tokio::try_join!(
            base(sea_orm_active_enums::ReportStatus::Pending).count(&self.db),
            base(sea_orm_active_enums::ReportStatus::Resolved).count(&self.db),
            base(sea_orm_active_enums::ReportStatus::Dismissed).count(&self.db),
        )?;
        Ok(ReportStatusCounts { pending, resolved, dismissed })
    }

    async fn list_all<'a>(
        &self,
        status: Option<ReportStatus>,
        target_type: Option<&'a str>,
        q: Option<&'a str>,
        category_ids: Option<&'a [Uuid]>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;
        let mut query = reports::Entity::find();

        if let Some(cats) = category_ids {
            query = scope_to_categories(query, cats);
        }

        if let Some(s) = status {
            let entity_status = match s {
                ReportStatus::Pending => sea_orm_active_enums::ReportStatus::Pending,
                ReportStatus::Resolved => sea_orm_active_enums::ReportStatus::Resolved,
                ReportStatus::Dismissed => sea_orm_active_enums::ReportStatus::Dismissed,
            };
            query = query.filter(reports::Column::Status.eq(entity_status));
        }

        match target_type {
            Some("post") => query = query.filter(reports::Column::PostId.is_not_null()),
            Some("thread") => query = query.filter(reports::Column::ThreadId.is_not_null()),
            _ => {}
        }

        if let Some(search) = q.filter(|s| !s.is_empty()) {
            let pattern = format!("%{}%", search.to_lowercase());
            query = query.filter(
                sea_orm::sea_query::Expr::col(reports::Column::Reason)
                    .like(pattern),
            );
        }

        let (total, rows) = tokio::try_join!(
            query.clone().count(&self.db),
            query
                .order_by_asc(reports::Column::CreatedAt)
                .limit(per_page)
                .offset(offset)
                .all(&self.db),
        )?;
        Ok((rows.into_iter().map(entity_to_domain).collect(), total))
    }

    async fn list_by_reporter(
        &self,
        reporter_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<Report>, u64), AppError> {
        let offset = page.saturating_sub(1) * per_page;
        let query = reports::Entity::find().filter(reports::Column::ReporterId.eq(reporter_id));

        let (total, rows) = tokio::try_join!(
            query.clone().count(&self.db),
            query
                .order_by_desc(reports::Column::CreatedAt)
                .limit(per_page)
                .offset(offset)
                .all(&self.db),
        )?;
        Ok((rows.into_iter().map(entity_to_domain).collect(), total))
    }

    async fn create(
        &self,
        reporter_id: Uuid,
        post_id: Option<Uuid>,
        thread_id: Option<Uuid>,
        reason: String,
    ) -> Result<Report, AppError> {
        let model = reports::ActiveModel {
            id: Set(Uuid::new_v4()),
            reporter_id: Set(reporter_id),
            post_id: Set(post_id),
            thread_id: Set(thread_id),
            reason: Set(reason),
            ..Default::default()
        };
        let inserted = model.insert(&self.db).await?;
        Ok(entity_to_domain(inserted))
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: ReportStatus,
        resolved_by_id: Uuid,
        moderator_notes: Option<String>,
    ) -> Result<(), AppError> {
        let entity_status = match status {
            ReportStatus::Pending => sea_orm_active_enums::ReportStatus::Pending,
            ReportStatus::Resolved => sea_orm_active_enums::ReportStatus::Resolved,
            ReportStatus::Dismissed => sea_orm_active_enums::ReportStatus::Dismissed,
        };

        let mut active = reports::ActiveModel {
            id: Set(id),
            status: Set(entity_status),
            resolved_by_id: Set(Some(resolved_by_id)),
            resolved_at: Set(Some(Utc::now().fixed_offset())),
            ..Default::default()
        };

        if let Some(notes) = moderator_notes {
            active.moderator_notes = Set(Some(notes));
        }

        active.update(&self.db).await?;
        Ok(())
    }
}
