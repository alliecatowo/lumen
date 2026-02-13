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
  subtotal: Float
  tax: Float
  total: Float
end

record AuditResult
  invoice_id: String
  status: String
  issue_count: Int
  summary: String
end

cell validate_totals(invoice: Invoice) -> list[String]
  let issues = []
  let expected_total = invoice.subtotal + invoice.tax
  if invoice.total != expected_total
    issues = append(issues, "Total mismatch: " + to_string(invoice.total) + " != " + to_string(expected_total))
  end
  if invoice.subtotal < 0.0
    issues = append(issues, "Negative subtotal: " + to_string(invoice.subtotal))
  end
  if invoice.tax < 0.0
    issues = append(issues, "Negative tax: " + to_string(invoice.tax))
  end
  return issues
end

cell audit_invoice(invoice: Invoice) -> AuditResult
  let issues = validate_totals(invoice)
  let status = "pass"
  if len(issues) > 0
    status = "fail"
  end
  let summary = "Validated " + invoice.vendor + " invoice " + invoice.id
  return AuditResult(invoice_id: invoice.id, status: status, issue_count: len(issues), summary: summary)
end

cell main() -> Null
  print("═══════════════════════════════════════")
  print("  🧾 Lumen Invoice Audit Agent")
  print("═══════════════════════════════════════")
  print("")

  let inv1 = Invoice(id: "INV-001", vendor: "Acme Corp", date: "2024-01-15", currency: "USD", subtotal: 1000.0, tax: 80.0, total: 1080.0)
  let inv2 = Invoice(id: "INV-002", vendor: "Beta Inc", date: "2024-01-16", currency: "EUR", subtotal: 500.0, tax: 50.0, total: 555.0)
  let inv3 = Invoice(id: "INV-003", vendor: "Gamma LLC", date: "2024-01-17", currency: "GBP", subtotal: 750.0, tax: 112.5, total: 862.5)

  let invoices = [inv1, inv2, inv3]
  let audit_results = []

  for inv in invoices
    print("Auditing " + inv.id + " from " + inv.vendor + "...")
    let audit = audit_invoice(inv)
    audit_results = append(audit_results, audit)
    match audit.status
      "pass" -> print("  ✅ " + audit.invoice_id + ": PASS")
      "fail" -> print("  ❌ " + audit.invoice_id + ": FAIL — " + to_string(audit.issue_count) + " issues")
      _ -> print("  ⚠️  " + audit.invoice_id + ": UNKNOWN")
    end
  end

  print("")
  print("─── Audit Summary ───")
  let pass_count = 0
  let fail_count = 0
  for r in audit_results
    if r.status == "pass"
      pass_count = pass_count + 1
    else
      fail_count = fail_count + 1
    end
  end
  print("  Passed: " + to_string(pass_count))
  print("  Failed: " + to_string(fail_count))
  print("  Total:  " + to_string(len(audit_results)))
  print("")
  print("All invoices audited. Trace recorded.")
  return null
end
```
