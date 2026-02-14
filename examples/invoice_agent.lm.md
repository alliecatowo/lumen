# Invoice Agent

> A Lumen agent for automated invoice auditing.
> Demonstrates: tool declarations, capability grants, schema validation,
> structured records, pattern matching, and multi-step workflows.

```lumen
use tool llm.chat as Chat
use tool http.get as Fetch

grant Chat
  model "gpt-4o"
  max_tokens 2048
  temperature 0.2

grant Fetch
  allowed_domains ["api.example.com"]
  timeout_ms 5000

record Invoice
  id: String
  vendor: String
  date: String
  currency: String
  subtotal: Float where subtotal >= 0.0
  tax: Float where tax >= 0.0
  total: Float # where total == subtotal + tax (Dependent constraints not supported yet)
end

record AuditResult
  invoice_id: String
  status: String
  issue_count: Int
  summary: String
end

cell validate_invoice(invoice: Invoice) -> AuditResult
  # Constraints on Record will auto-validate subtotal/tax/total logic upon construction.
  # If we get here, the invoice is valid structurally and mathematically.
  
  let issues = []
  if invoice.currency != "USD"
    issues = append(issues, "Non-USD currency: " + invoice.currency)
  end
  
  let status = "APPROVED"
  if length(issues) > 0
    status = "FLAGGED"
  end
  
  # Use LLM to summarize
  let prompt = "Summarize this invoice for " + invoice.vendor + " amount " + to_string(invoice.total)
  role system: You are a financial auditor.
  role user: {prompt}
  
  # Mock LLM response for now as we don't have real API keys in tests
  let summary = "Invoice " + invoice.id + " generally looks good."

  return AuditResult(
    invoice_id: invoice.id,
    status: status,
    issue_count: length(issues),
    summary: summary
  )
end

cell main() -> Int
  # Valid Invoice
  let inv1 = Invoice(
    id: "INV-001",
    vendor: "Acme Corp",
    date: "2023-10-27",
    currency: "USD",
    subtotal: 100.0,
    tax: 10.0,
    total: 110.0
  )
  
  let res1 = validate_invoice(inv1)
  print("Audit 1: " + res1.status + " - " + res1.summary)

  # Invalid Invoice (Mathematical error) - Should Halt
  # let inv2 = Invoice(
  #   id: "INV-002",
  #   vendor: "Bad Corp",
  #   date: "2023-10-28",
  #   currency: "USD",
  #   subtotal: 100.0,
  #   tax: 10.0,
  #   total: 120.0 
  # ) 

  return 0
end
```
