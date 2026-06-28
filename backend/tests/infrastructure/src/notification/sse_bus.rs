use uuid::Uuid;

use ferum_infrastructure::notification::sse_bus::SseBroadcaster;

#[tokio::test]
async fn subscribe_and_publish_delivers_message_immediately() {
    let bus = SseBroadcaster::new();
    let user_id = Uuid::new_v4();
    let mut rx = bus.subscribe(user_id);
    bus.publish(user_id, r#"{"kind":"mention"}"#.to_string());
    let msg = rx.try_recv().expect("message must be available immediately after publish");
    assert_eq!(msg, r#"{"kind":"mention"}"#);
}

#[tokio::test]
async fn publish_to_unsubscribed_user_is_noop() {
    let bus = SseBroadcaster::new();
    bus.publish(Uuid::new_v4(), "data".to_string());
}

#[tokio::test]
async fn multiple_subscribers_for_same_user_all_receive() {
    let bus = SseBroadcaster::new();
    let user_id = Uuid::new_v4();
    let mut rx1 = bus.subscribe(user_id);
    let mut rx2 = bus.subscribe(user_id);
    bus.publish(user_id, "hello".to_string());
    assert_eq!(rx1.try_recv().unwrap(), "hello");
    assert_eq!(rx2.try_recv().unwrap(), "hello");
}

#[tokio::test]
async fn active_connection_count_tracks_all_subscriptions() {
    let bus = SseBroadcaster::new();
    assert_eq!(bus.active_connection_count(), 0);
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    let _rx1 = bus.subscribe(user_a);
    let _rx2 = bus.subscribe(user_b);
    let _rx3 = bus.subscribe(user_a);
    assert_eq!(bus.active_connection_count(), 3);
}

#[tokio::test]
async fn closed_receiver_is_pruned_on_next_subscribe() {
    let bus = SseBroadcaster::new();
    let user_id = Uuid::new_v4();
    {
        let _rx = bus.subscribe(user_id);
        // _rx dropped here — sender becomes closed
    }
    let _rx2 = bus.subscribe(user_id);
    assert_eq!(bus.active_connection_count(), 1);
}
