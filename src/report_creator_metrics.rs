use creator_commerce_metrics::{report_workflow, BusinessEvent, InfraiClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event = BusinessEvent { creator_id: "creator-7".into(), asset_id: "pack-22".into(), subscribers: 1000, delivered: 995, processed: 990 };
    let client = InfraiClient::from_env()?;
    let result = report_workflow(&client, &event).await?;
    println!("creator={} asset={} delivery_rate={:.3} processing_rate={:.3} healthy={}", event.creator_id, event.asset_id, result.delivery_rate, result.processing_rate, result.healthy);
    Ok(())
}
