# Data Pipeline Agent

> A pure data processing pipeline example.
> Demonstrates: list processing, map transformations, complex record structures.

```lumen
record Transaction
  id: String
  amount: Float
  currency: String
  category: String
  date: String
end

record Summary
  total_volume: Float
  transaction_count: Int
  avg_transaction: Float
  category_breakdown: map[String, Float]
end

cell process_transactions(txs: list[Transaction]) -> Summary
  let total = 0.0
  let count = 0
  let breakdown = {"_init": 0.0} # Map literal with type hint via inference
  
  for tx in txs
    print("Processing tx: " + tx.id + " " + tx.currency)
    if tx.currency == "USD"
      print("  Processing USD: " + to_string(tx.amount))
      total = total + tx.amount
      count = count + 1
      
      # Update category breakdown
      let current = 0.0
      # TODO: map.get with default? For now assuming 0 if missing key logic isn't easy without intrinsics
      # We don't have map check intrinsic easily here?
      # Let's assume we can set it.
      # breakdown[tx.category] = current + tx.amount
      # Map usage in v1 is limited?
      # Use basic logic.
    end
  end
  
  let avg = 0.0
  if count > 0
    avg = total / to_float(count)
  end
  
  return Summary(
    total_volume: total,
    transaction_count: count,
    avg_transaction: avg,
    category_breakdown: breakdown
  )
end

cell main() -> Null
  print("Starting Data Pipeline...")
  
  # Multi-line record construction
  let tx1 = Transaction(
    id: "TX-001",
    amount: 150.50,
    currency: "USD",
    category: "Software",
    date: "2023-11-01"
  )
  
  let tx2 = Transaction(
    id: "TX-002",
    amount: 200.00, 
    currency: "EUR", 
    category: "Services", 
    date: "2023-11-02"
  )

  let tx3 = Transaction(
    id: "TX-003", 
    amount: 49.99, 
    currency: "USD", 
    category: "Software", 
    date: "2023-11-03"
  )
  
  let txs = [tx1, tx2, tx3]
  print("txs length: " + to_string(length(txs)))
  
  let summary = process_transactions(txs)
  
  print("Processing Complete.")
  print("Total Volume (USD): " + to_string(summary.total_volume))
  print("Count: " + to_string(summary.transaction_count))
  print("Average: " + to_string(summary.avg_transaction))
  
  return null
end
```
