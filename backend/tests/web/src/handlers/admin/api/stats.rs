use ferum_web::handlers::admin::api::stats::StatsHistoryQuery;

#[test]
fn stats_history_query_all_optional() {
    let q: StatsHistoryQuery = serde_json::from_str(r#"{}"#).unwrap();
    assert!(q.metric.is_none());
    assert!(q.days.is_none());
}

#[test]
fn stats_history_query_with_metric_and_days() {
    let q: StatsHistoryQuery =
        serde_json::from_str(r#"{"metric":"dau","days":30}"#).unwrap();
    assert_eq!(q.metric.as_deref(), Some("dau"));
    assert_eq!(q.days, Some(30));
}

#[test]
fn stats_history_query_known_metrics_deserialize() {
    for metric in &["dau", "mau", "new_users", "new_threads", "new_posts"] {
        let json = format!(r#"{{"metric":"{metric}"}}"#);
        let q: StatsHistoryQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(q.metric.as_deref(), Some(*metric));
    }
}
