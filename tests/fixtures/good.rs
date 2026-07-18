pub async fn load_customer(
    customer_id: u64,
    correlation_id: &str,
) -> Result<(), DatabaseError> {
    error!(
        operation = "load_customer",
        customer_id = customer_id,
        correlation_id = correlation_id,
        "primary database timed out"
    );
    return Err(DatabaseError);
}
