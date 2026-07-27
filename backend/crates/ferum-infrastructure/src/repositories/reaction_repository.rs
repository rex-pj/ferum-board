
use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::prelude::*;
use sea_orm::sea_query::{Alias, Expr, PostgresQueryBuilder, Query};
use sea_orm::*;
use uuid::Uuid;

use crate::entities::{reactions, sea_orm_active_enums};
use ferum_application::shared::AppError;
use ferum_domain::models::reaction::{Reaction, ReactionKind};
use ferum_domain::repositories::reaction_repository::ReactionRepository;

pub struct PgReactionRepository {
    db: DatabaseConnection,
}

impl PgReactionRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn kind_to_entity(k: &ReactionKind) -> sea_orm_active_enums::ReactionKind {
    match k {
        ReactionKind::Like => sea_orm_active_enums::ReactionKind::Like,
        ReactionKind::Helpful => sea_orm_active_enums::ReactionKind::Helpful,
        ReactionKind::Insightful => sea_orm_active_enums::ReactionKind::Insightful,
        ReactionKind::Funny => sea_orm_active_enums::ReactionKind::Funny,
    }
}

fn entity_to_kind(k: &sea_orm_active_enums::ReactionKind) -> ReactionKind {
    match k {
        sea_orm_active_enums::ReactionKind::Like => ReactionKind::Like,
        sea_orm_active_enums::ReactionKind::Helpful => ReactionKind::Helpful,
        sea_orm_active_enums::ReactionKind::Insightful => ReactionKind::Insightful,
        sea_orm_active_enums::ReactionKind::Funny => ReactionKind::Funny,
    }
}

fn entity_to_domain(m: reactions::Model) -> Reaction {
    Reaction {
        post_id: m.post_id,
        user_id: m.user_id,
        kind: entity_to_kind(&m.kind),
        created_at: m.created_at.with_timezone(&Utc),
    }
}

#[async_trait]
impl ReactionRepository for PgReactionRepository {
    async fn find(
        &self,
        post_id: Uuid,
        user_id: Uuid,
        kind: ReactionKind,
    ) -> Result<Option<Reaction>, AppError> {
        Ok(reactions::Entity::find()
            .filter(reactions::Column::PostId.eq(post_id))
            .filter(reactions::Column::UserId.eq(user_id))
            .filter(reactions::Column::Kind.eq(kind_to_entity(&kind)))
            .one(&self.db)
            .await?
            .map(entity_to_domain))
    }

    async fn counts_by_post(&self, post_id: Uuid) -> Result<Vec<(ReactionKind, u64)>, AppError> {
        #[derive(Debug, sea_orm::FromQueryResult)]
        struct KindCount {
            kind: String,
            count: i64,
        }

        let (sql, values) = Query::select()
            .expr_as(
                Expr::col(reactions::Column::Kind).cast_as(Alias::new("text")),
                Alias::new("kind"),
            )
            .expr_as(
                Expr::cust("COUNT(*)::BIGINT"),
                Alias::new("count"),
            )
            .from(reactions::Entity)
            .and_where(Expr::col(reactions::Column::PostId).eq(post_id))
            .group_by_col(reactions::Column::Kind)
            .build(PostgresQueryBuilder);

        let rows = KindCount::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .all(&self.db)
        .await?;

        let result = rows
            .into_iter()
            .filter_map(|r| {
                let kind: ReactionKind = r.kind.parse().ok()?;
                Some((kind, r.count as u64))
            })
            .collect();

        Ok(result)
    }

    async fn counts_by_posts(
        &self,
        post_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<(ReactionKind, u64)>>, AppError> {
        if post_ids.is_empty() {
            return Ok(HashMap::new());
        }

        #[derive(Debug, sea_orm::FromQueryResult)]
        struct PostKindCount {
            post_id: Uuid,
            kind: String,
            count: i64,
        }

        let (sql, values) = Query::select()
            .column(reactions::Column::PostId)
            .expr_as(
                Expr::col(reactions::Column::Kind).cast_as(Alias::new("text")),
                Alias::new("kind"),
            )
            .expr_as(
                Expr::cust("COUNT(*)::BIGINT"),
                Alias::new("count"),
            )
            .from(reactions::Entity)
            .and_where(Expr::col(reactions::Column::PostId).is_in(post_ids.to_vec()))
            .group_by_col(reactions::Column::PostId)
            .group_by_col(reactions::Column::Kind)
            .build(PostgresQueryBuilder);

        let rows = PostKindCount::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .all(&self.db)
        .await?;

        let mut result: HashMap<Uuid, Vec<(ReactionKind, u64)>> = HashMap::new();
        for row in rows {
            if let Ok(kind) = row.kind.parse::<ReactionKind>() {
                result
                    .entry(row.post_id)
                    .or_default()
                    .push((kind, row.count as u64));
            }
        }
        Ok(result)
    }

    async fn user_reactions_for_posts(
        &self,
        user_id: Uuid,
        post_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<ReactionKind>>, AppError> {
        if post_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = reactions::Entity::find()
            .filter(reactions::Column::UserId.eq(user_id))
            .filter(reactions::Column::PostId.is_in(post_ids.to_vec()))
            .all(&self.db)
            .await?;

        let mut result: HashMap<Uuid, Vec<ReactionKind>> = HashMap::new();
        for row in rows {
            result
                .entry(row.post_id)
                .or_default()
                .push(entity_to_kind(&row.kind));
        }
        Ok(result)
    }

    async fn add(
        &self,
        post_id: Uuid,
        user_id: Uuid,
        kind: ReactionKind,
    ) -> Result<Reaction, AppError> {
        let model = reactions::ActiveModel {
            post_id: Set(post_id),
            user_id: Set(user_id),
            kind: Set(kind_to_entity(&kind)),
            ..Default::default()
        };
        let inserted = model.insert(&self.db).await.map_err(|e| {
            if e.to_string().contains("unique") || e.to_string().contains("duplicate") {
                AppError::Conflict("reaction_exists".to_string())
            } else {
                AppError::from(e)
            }
        })?;
        Ok(entity_to_domain(inserted))
    }

    async fn remove(
        &self,
        post_id: Uuid,
        user_id: Uuid,
        kind: ReactionKind,
    ) -> Result<(), AppError> {
        reactions::Entity::delete_many()
            .filter(reactions::Column::PostId.eq(post_id))
            .filter(reactions::Column::UserId.eq(user_id))
            .filter(reactions::Column::Kind.eq(kind_to_entity(&kind)))
            .exec(&self.db)
            .await?;
        Ok(())
    }
}
