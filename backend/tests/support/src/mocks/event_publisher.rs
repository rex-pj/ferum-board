use async_trait::async_trait;
use ferum_application::event_bus::EventPublisher;
use ferum_domain::events::ForumEvent;

mockall::mock! {
    pub EventPublisher {}

    #[async_trait]
    impl EventPublisher for EventPublisher {
        async fn publish(&self, event: ForumEvent);
    }
}
