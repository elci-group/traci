pub async fn load_customer(customer_id: u64) -> Result<(), DatabaseError> {
    let _ = customer_id;
    cache_write().ok();
    tokio::spawn(async move { refresh_cache().await; });
    return Err(DatabaseError);
}
