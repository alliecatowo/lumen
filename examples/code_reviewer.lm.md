# Code Reviewer Agent

> A multi-cell pipeline for automated code review.
> Demonstrates: tool grants with constraints, enum-based scoring,
> multi-step agent pipelines, and rich record types.

```lumen
use tool llm.chat as Reviewer
use tool http.get as GitHub

grant Reviewer
  model "gpt-4o"
  max_tokens 4096
  temperature 0.3

grant GitHub
  allowed_domains ["api.github.com"]
  timeout_ms 10000

enum Severity
  Info
  Warning
  Error
  Critical
end

enum Grade
  Excellent
  Good
  NeedsWork
  Reject
end

record CodeIssue
  file: String
  line: Int
  severity: String
  message: String
end

record ReviewResult
  grade: String
  score: Int
  issue_count: Int
  summary: String
end

record PullRequest
  repo: String
  number: Int
  title: String
  author: String
  additions: Int
  deletions: Int
end

cell make_issue(file: String, line: Int, severity: String, message: String) -> CodeIssue
  return CodeIssue(file: file, line: line, severity: severity, message: message)
end

cell count_by_severity(issues: list[CodeIssue], sev: String) -> Int
  let count = 0
  for issue in issues
    if issue.severity == sev
      count = count + 1
    end
  end
  return count
end

cell compute_grade(issues: list[CodeIssue]) -> String
  let critical = count_by_severity(issues, "Critical")
  let errors = count_by_severity(issues, "Error")
  let warnings = count_by_severity(issues, "Warning")
  if critical > 0
    return "Reject"
  end
  if errors > 2
    return "NeedsWork"
  end
  if errors > 0
    return "Good"
  end
  return "Excellent"
end

cell compute_score(issues: list[CodeIssue]) -> Int
  let score = 100
  for issue in issues
    if issue.severity == "Critical"
      score = score - 25
    end
    if issue.severity == "Error"
      score = score - 10
    end
    if issue.severity == "Warning"
      score = score - 5
    end
    if issue.severity == "Info"
      score = score - 1
    end
  end
  if score < 0
    return 0
  end
  return score
end

cell format_severity_icon(severity: String) -> String
  match severity
    "Critical" -> return "🔴"
    "Error" -> return "🟠"
    "Warning" -> return "🟡"
    "Info" -> return "🔵"
    _ -> return "⚪"
  end
end

cell format_grade_icon(grade: String) -> String
  match grade
    "Excellent" -> return "🏆"
    "Good" -> return "✅"
    "NeedsWork" -> return "⚠️"
    "Reject" -> return "❌"
    _ -> return "❓"
  end
end

cell summarize_review(pr: PullRequest, grade: String, score: Int) -> String
  # Use LLM to generate a summary
  # We use role blocks here
  role system: You are a helpful code review assistant.
  role user: Summarize the review for PR {pr.number} ({pr.title}). Grade: {grade}, Score: {score}.
  
  # Mock response
  return "PR #" + to_string(pr.number) + " received a grade of " + grade + " (" + to_string(score) + "/100)."
end

cell main() -> Null
  print("═══════════════════════════════════════════")
  print("  🔍 Lumen Code Reviewer Agent")
  print("═══════════════════════════════════════════")
  print("")

  let pr = PullRequest(repo: "alliecatowo/lumen", number: 42, title: "Add string interpolation", author: "alice", additions: 350, deletions: 120)
  print("Reviewing PR #" + to_string(pr.number) + ": " + pr.title)
  print("Author: " + pr.author)
  print("Changes: +" + to_string(pr.additions) + " / -" + to_string(pr.deletions))
  print("")

  let issues = []
  let i1 = make_issue("src/parser.rs", 42, "Warning", "Complex function exceeds 50 lines")
  issues = append(issues, i1)
  let i2 = make_issue("src/lexer.rs", 15, "Info", "Consider extracting helper function")
  issues = append(issues, i2)
  let i3 = make_issue("src/vm.rs", 230, "Error", "Missing bounds check on register access")
  issues = append(issues, i3)
  let i4 = make_issue("src/lower.rs", 88, "Warning", "Unused variable 'tmp_reg'")
  issues = append(issues, i4)

  let grade = compute_grade(issues)
  let score = compute_score(issues)
  let summary = summarize_review(pr, grade, score)

  print("─── Review Results ───")
  print("  Grade: " + format_grade_icon(grade) + " " + grade)
  print("  Score: " + to_string(score) + "/100")
  print("  Summary: " + summary)
  print("")

  print("Issues (" + to_string(length(issues)) + "):")
  for issue in issues
    let icon = format_severity_icon(issue.severity)
    print("  " + icon + " " + issue.file + ":" + to_string(issue.line))
    print("    " + issue.message)
  end

  print("")
  print("Review complete.")
  return null
end
```
