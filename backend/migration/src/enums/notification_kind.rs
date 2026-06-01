use sea_orm_migration::prelude::*;

#[derive(Iden)]
pub enum NotificationKindEnum {
    #[iden = "notification_kind"]
    Type,
    #[iden = "reply"]
    Reply,
    #[iden = "mention"]
    Mention,
    #[iden = "reaction"]
    Reaction,
    #[iden = "best_answer"]
    BestAnswer,
    #[iden = "warn"]
    Warn,
    #[iden = "system"]
    System,
}
