use crate::card_provider::CardProvider;

pub async fn healthcheck(provider: CardProvider) {
    check_dependencies(&provider).await;
}

async fn check_dependencies(provider: &CardProvider) {
    for card in provider.load_all().await {
        for dep in card.dependency_ids().await {
            let _card = provider.load(dep).await;
            if _card.is_none() {
                tracing::error!("{card}s dependency {dep} not found");
            }
        }
    }
}
