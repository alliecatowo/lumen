# Domain Models

Shared workspace/domain types for an AI-assisted engineering pipeline.

```lumen
type WorkId = String

record WorkspaceContext
  workspace: String
  tenant: String
  mode: String
end

record WorkItem
  id: WorkId
  title: String
  owner: String
  tags: list[String]
  effort: Int
end

record WorkQueue
  context: WorkspaceContext
  items: list[WorkItem]
end
```
