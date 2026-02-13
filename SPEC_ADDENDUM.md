# The Lumen Language Specification — V2 Addendum

**Version 2.0 — The Agent-Complete Edition**
**Status: Extends V1 Specification**

> V1 defined the language. V2 makes it the definitive agent programming model —
> with typed effects, content-addressed definitions, first-class agents,
> orchestration primitives, memory, guardrails, and formal semantics.

---

# Part I: Typed Effect System

> *"If the type system doesn't track effects, the capability model is a runtime prayer, not a compile-time proof."*

---

## 1. Effect Rows

Every cell in Lumen V2 declares not only its input and output types but also its **effect row** — a compile-time record of every side effect the cell may perform. Effect rows are inspired by Koka's row-polymorphic effect types (Leijen, 2014) adapted for Lumen's tool-and-trace model.

### 1.1 Syntax

The effect row follows the return type, separated by `/`:

```lumen
cell pure_add(a: Int, b: Int) -> Int / {}
cell fetch(url: String) -> Bytes / {http}
cell extract(text: String) -> Invoice / {llm, http, trace}
cell risky(url: String) -> Bytes / {http, fs}
```

- `/ {}` — **total**: no effects. Guaranteed pure. The compiler proves it.
- `/ {http}` — may perform HTTP effects.
- `/ {llm, http, trace}` — may call an LLM, perform HTTP, and emit trace events.

If the effect row is omitted, the compiler **infers** it from the cell body. Explicit annotations are optional but recommended for public APIs.

### 1.2 Effect Kinds

Lumen defines a hierarchy of built-in effect kinds:

| Effect | Triggered By | Description |
|--------|-------------|-------------|
| `pure` | (nothing) | Alias for `{}`. Mathematically total. |
| `http` | `HttpGet`, `HttpPost`, any HTTP tool | Network I/O |
| `llm` | `Chat`, any LLM tool | Language model invocation |
| `fs` | `ReadFile`, `WriteFile` | Filesystem access |
| `mcp` | Any MCP tool call | External MCP server interaction |
| `trace` | Trace emission | Writing to the trace log |
| `cache` | Cache read/write | Interacting with the cache store |
| `emit` | `emit()` | Producing user-visible output |
| `state` | Mutable variable access | Reading/writing mutable state |
| `time` | `timestamp()`, `sleep()` | Time-dependent operations |
| `random` | `random_int()`, `uuid()` | Non-deterministic values |
| `approve` | `approve` blocks | Human-in-the-loop |
| `diverge` | Potentially non-terminating loops | May not terminate |

### 1.3 User-Defined Effects

Users can declare custom effect kinds tied to their tools:

```lumen
effect database
  cell query(sql: String) -> list[Json]
  cell execute(sql: String) -> Int
end

effect email
  cell send(to: String, subject: String, body: String) -> Bool
end

# Tool binding
use tool postgres.query as DbQuery
bind effect database.query to DbQuery

# Now cells using DbQuery have {database} in their effect row
cell get_users() -> list[User] / {database}
  let rows = DbQuery(sql: "SELECT * FROM users")
  return rows.map(fn(r) => r.as_schema(User).unwrap())
end
```

### 1.4 Effect Polymorphism

Cells can be polymorphic over effects, enabling higher-order functions that preserve effect information:

```lumen
# map preserves whatever effects f has
cell map_list[T, U, E](items: list[T], f: fn(T) -> U / E) -> list[U] / E
  return [f(item) for item in items]
end

# This correctly infers that the result has {http} effects
let urls = ["https://a.com", "https://b.com"]
let results = map_list(urls, fn(u) => HttpGet(url: u))
# inferred: list[Response] / {http}
```

The effect variable `E` is a **row variable** — it stands for any set of effects. When `map_list` is called with a pure function, the result is pure. When called with an effectful function, the result carries those effects. This is not magic; it's row polymorphism applied to effects, the same mechanism Koka uses.

### 1.5 Effect Subtyping

Effects form a natural subtyping relationship: a cell with fewer effects can be used wherever a cell with more effects is expected.

```lumen
# A pure cell can be used where an effectful cell is expected
cell double(x: Int) -> Int / {}
  return x * 2
end

# This accepts any fn(Int) -> Int with any effects
cell apply[E](f: fn(Int) -> Int / E, x: Int) -> Int / E
  return f(x)
end

apply(double, 5)  # works: {} ⊆ E for any E
```

Formally: `{a, b} ⊆ {a, b, c}`. A cell with effect row `{http}` can be used where `{http, trace}` is expected, because it does *fewer* things than allowed.

### 1.6 Effect Handlers

Effect handlers allow cells to intercept and reinterpret effects. This is the mechanism that makes testing, mocking, and sandboxing composable:

```lumen
# Define a handler that intercepts HTTP effects
handler MockHttp
  handle http.get(url: String) -> Response
    # Instead of making a real HTTP call, return mock data
    return Response(status: 200, body: mock_data_for(url))
  end
end

# Run a cell with the mock handler — no real HTTP calls
cell test_extraction()
  with MockHttp
    let result = extract_invoice("test input")
    assert(result.is_ok())
  end
end
```

Effect handlers are the principled replacement for dependency injection, mocking frameworks, and test doubles. They compose: you can stack handlers, and inner handlers shadow outer ones.

```lumen
# Compose handlers
with MockHttp, MockLlm, TraceToMemory
  let result = full_pipeline(input)
  # All HTTP calls mocked, all LLM calls mocked, traces go to memory
end
```

### 1.7 Effect Erasure and Optimization

The compiler uses effect information for optimization:

- **Pure cells** (`/ {}`) can be memoized automatically, reordered, or evaluated at compile time.
- **Cells with only `{trace}`** can have tracing stripped in release builds.
- **Cells with only `{state}`** can be optimized with register allocation for the mutable variables.
- **Parallel blocks** require branches to have non-overlapping effect rows (no two branches can both write to `{fs}` unless they write to different paths).

### 1.8 Effect Row Inference Rules

The compiler infers effect rows bottom-up:

1. Literal expressions, arithmetic, and pattern matching: `{}`
2. Tool calls: the tool's declared effect kind (e.g., `HttpGet` → `{http}`)
3. `let` bindings: the effect of the bound expression
4. Sequential composition: union of all statement effects
5. `if`/`match`: union of all branch effects
6. `for`/`while`/`loop`: body effects ∪ `{diverge}` (unless termination is proven)
7. Cell calls: the called cell's declared or inferred effect row
8. `parallel` blocks: union of all branch effects
9. Closures: captured effects plus body effects

### 1.9 The Pure Guarantee

A cell annotated `/ {}` or `@pure` is verified by the compiler to:

- Call no tools
- Perform no I/O
- Access no mutable state
- Emit no output
- Reference no time or randomness
- Call only other pure cells

If any of these are violated, it's a compile error. This is not a convention — it's a proof.

### 1.10 Effect-Capability Bridge

Effects and capabilities are unified. The effect row determines what grants a cell needs:

```lumen
cell fetch_data(url: String) -> Bytes / {http}
  let response = HttpGet(url: url)
  return response.body
end

# The compiler checks: fetch_data has {http} in its effect row,
# therefore it MUST be called from a context that has a grant for HttpGet.
# If no grant is in scope, compile error:
#   error[E0401]: cell `fetch_data` requires effect `http` but no
#   grant for an HTTP tool is in scope
```

This closes the gap between "the type says it's safe" and "the runtime enforces it." In V2, the type system IS the capability system.

---

## 2. Content-Addressed Definitions

Inspired by Unison (shipped 1.0, November 2025), Lumen V2 content-addresses all definitions — not just traces and blobs.

### 2.1 Definition Hashing

Every cell, record, enum, trait, and type alias is assigned a **definition hash** — a SHA-256 hash of its normalized AST. The hash is computed over:

1. The structure of the code (AST nodes)
2. The hashes of all referenced definitions (transitively)
3. The types of all parameters and return values
4. The effect row
5. Constraint expressions

The hash does NOT include:

- Human-readable names (names are metadata, not identity)
- Comments and documentation
- Whitespace and formatting
- Source location

```lumen
# These two cells have the SAME definition hash:
cell double(x: Int) -> Int / {} = x * 2
cell multiply_by_two(n: Int) -> Int / {} = n * 2

# This one has a DIFFERENT hash (different AST structure):
cell double(x: Int) -> Int / {} = x + x
```

### 2.2 Implications

**Perfect incremental compilation.** If a cell's definition hash hasn't changed, its LIR doesn't need to be regenerated. The compiler maintains a hash → LIR cache. On a typical edit, only the changed cell and its dependents need recompilation.

**Correct cache invalidation.** Tool call cache keys in V2 include the calling cell's definition hash:

```
cache_key_v2 = sha256(
  tool_id + ":" +
  tool_version + ":" +
  policy_hash + ":" +
  calling_cell_hash + ":" +   # NEW in V2
  canonical_json(args)
)
```

This means: if you change the cell's logic but the tool inputs happen to be the same, the cache correctly misses. V1 couldn't guarantee this.

**No diamond dependency conflicts.** Two packages can depend on different versions of the same library. Because definitions are identified by hash, not by name, both versions coexist without conflict. The runtime loads whichever version each dependent actually references.

**Semantic diffing.** `lumen diff` can compare two versions of a module and report exactly which definitions changed semantically (not just textually). Renamed cells with the same implementation are correctly identified as unchanged.

### 2.3 The Definition Store

The compiler maintains a local definition store:

```
.lumen/
  defs/
    sha256/<hash>.lir      # compiled LIR for each definition
    sha256/<hash>.meta      # name, docs, source location, effect row
  index/
    names.json              # name → hash mapping (current version)
    reverse.json            # hash → [names] mapping
```

### 2.4 Definition Identity in Traces

Trace events now include the definition hash of the cell that produced them:

```json
{
  "seq": 7,
  "kind": "cell_enter",
  "cell_name": "extract",
  "cell_hash": "sha256:a1b2c3...",
  ...
}
```

This makes traces fully self-contained — you can verify that a trace was produced by a specific version of the code, even if the source has since been modified.

### 2.5 Distributed Definition Transfer

When a Lumen program runs in a distributed context (V3+), definitions can be transferred between nodes by hash. A node that receives a `CALL` instruction for an unknown hash can request the definition from the originating node. This is the foundation for "programs that deploy themselves" (the Unison model applied to agent workflows).

---

# Part II: First-Class Agents

> *Every production agent framework converged on the same abstraction: role + tools + memory + state. This should be a language construct, not a pattern.*

---

## 3. The Agent Declaration

### 3.1 Syntax

```lumen
agent <Name> [type_params]
  [role: <role_block>]
  [tools: <tool_list>]
  [memory: <memory_list>]
  [guardrails: <guardrail_list>]
  [config: <config_block>]

  [grant declarations]
  [cell declarations]
end
```

### 3.2 Complete Example

```lumen
agent InvoiceExtractor
  role:
    You are a specialist in extracting structured invoice data from
    unstructured text. You return only valid JSON matching the Invoice
    schema. You never fabricate data — if a field is not present in
    the source text, you set it to null.
  end

  tools: [Chat, HttpGet]
  memory: [ConversationMemory]
  guardrails: [NoPII, SchemaCompliance]

  config:
    model = "claude-sonnet-4-20250514"
    temperature = 0.0
    max_tokens = 4096
    max_retries = 3
    timeout_ms = 30000
  end

  grant Chat
    model config.model
    max_tokens config.max_tokens
    temperature config.temperature

  grant HttpGet
    domain "*.example.com"
    timeout_ms 5000

  ## Extract invoice from raw text
  cell extract(text: String) -> result[Invoice, ExtractionError] / {llm, trace}
    let response = Chat(
      role system: self.role,
      role user:
        Extract the invoice from this text:
        {text}
      end
    ) expect schema Invoice

    return response
  end

  ## Extract from a URL
  cell extract_from_url(url: String) -> result[Invoice, ExtractionError] / {http, llm, trace}
    let response = HttpGet(url: url)
    return self.extract(response.body.to_string())
  end
end
```

### 3.3 Agent Semantics

An agent declaration compiles to:

1. A **record type** containing the agent's configuration, memory references, and state
2. A **capability scope** containing all tool grants
3. A **namespace** containing all the agent's cells
4. **Lifecycle hooks** for initialization, shutdown, and error recovery

Agents are instantiated:

```lumen
let extractor = InvoiceExtractor()
let result = extractor.extract("Invoice #12345...")

# Or with config overrides
let fast_extractor = InvoiceExtractor(config: {
  model: "claude-haiku-4-5-20251001",
  timeout_ms: 5000
})
```

### 3.4 Agent Self-Reference

Inside an agent's cells, `self` refers to the agent instance:

```lumen
agent Assistant
  role:
    You are a helpful assistant named {self.config.name}.
  end

  config:
    name = "Lumen Assistant"
  end

  cell greet() -> String / {llm}
    return Chat(
      role system: self.role,
      role user: "Say hello"
    )
  end
end
```

### 3.5 Agent Composition

Agents can reference other agents as collaborators:

```lumen
agent Researcher
  # ...
  cell search(query: String) -> list[Paper] / {http}
    # ...
  end
end

agent Writer
  collaborators: [Researcher]

  cell write_report(topic: String) -> String / {llm, http}
    let papers = self.collaborators.Researcher.search(topic)
    return Chat(
      role system: self.role,
      role user: "Write a report based on: {papers}"
    )
  end
end
```

### 3.6 Agent Traits

Agents can implement traits for shared behavior:

```lumen
trait Conversable
  cell respond(message: String) -> String / {llm}
end

trait Evaluatable
  cell evaluate(input: String, expected: String) -> Score / {llm}
end

agent Reviewer
  impl Conversable
    cell respond(message: String) -> String / {llm}
      return Chat(role system: self.role, role user: message)
    end
  end

  impl Evaluatable
    cell evaluate(input: String, expected: String) -> Score / {llm}
      # ...
    end
  end
end
```

### 3.7 Agent Lifecycle

```lumen
agent StatefulAgent
  state:
    let mut interaction_count: Int = 0
    let mut last_topic: String | Null = null
  end

  ## Called when the agent is instantiated
  @on_init
  cell initialize() / {trace}
    emit("Agent initialized")
  end

  ## Called before each cell execution
  @on_before_call
  cell before(cell_name: String, args: Json) / {trace, state}
    self.state.interaction_count += 1
  end

  ## Called after each cell execution
  @on_after_call
  cell after(cell_name: String, result: Json) / {trace, state}
    # Update memory, log metrics, etc.
  end

  ## Called on unrecoverable error
  @on_error
  cell handle_error(error: LumenError) / {trace, emit}
    emit("Error in interaction {self.state.interaction_count}: {error.message}")
  end

  ## Called when the agent is destroyed
  @on_shutdown
  cell cleanup() / {trace}
    emit("Agent shutting down after {self.state.interaction_count} interactions")
  end
end
```

---

## 4. Multi-Agent Orchestration

### 4.1 Orchestration Patterns

Lumen provides first-class orchestration constructs for the patterns that every production framework has converged on.

#### 4.1.1 Pipeline (Sequential Chain)

```lumen
pipeline InvoicePipeline
  description: "Extract, validate, enrich, and store invoices"

  stages:
    Extractor.extract
      -> Validator.validate
      -> Enricher.add_metadata
      -> Writer.store
  end

  on_error(stage, error):
    match stage
      "validate" -> retry(max: 2)
      "store"    -> checkpoint_and_alert(error)
      _          -> halt(error.message)
    end
  end
end

# Usage
let result = InvoicePipeline.run(raw_text)
```

Pipelines are typed end-to-end. The compiler verifies that each stage's output type matches the next stage's input type. Effect rows accumulate through the pipeline.

#### 4.1.2 Coordinator-Worker (Fan-Out/Fan-In)

```lumen
orchestration ResearchTeam
  coordinator: Manager
  workers: [Researcher, Analyst, Writer]
  strategy: delegate_and_synthesize

  cell run(topic: String) -> Report / {llm, http, trace}
    # Coordinator breaks down the task
    let tasks = Manager.plan(topic)

    # Workers execute in parallel
    let results = await parallel for task in tasks
      match task.type
        "research"  -> Researcher.search(task.query)
        "analyze"   -> Analyst.analyze(task.data)
        "write"     -> Writer.draft(task.outline)
      end
    end

    # Coordinator synthesizes
    return Manager.synthesize(results)
  end
end
```

#### 4.1.3 Debate / Adversarial

```lumen
orchestration QualityDebate
  proposer: Optimist
  critic: Skeptic
  arbiter: Judge

  cell deliberate(proposal: String) -> Decision / {llm, trace}
    let mut current = proposal
    let mut round = 0

    while round < 3
      let critique = Skeptic.critique(current)

      if critique.severity == "none"
        break
      end

      let defense = Optimist.defend(current, critique)
      current = defense.revised_proposal
      round += 1
    end

    return Judge.decide(current, history: trace_ref())
  end
end
```

#### 4.1.4 Evaluator-Optimizer Loop

```lumen
orchestration RefinementLoop[T]
  generator: Agent
  evaluator: Agent
  max_iterations: Int = 5
  quality_threshold: Float = 0.8

  cell refine(prompt: String) -> T / {llm, trace}
    let mut output = generator.generate(prompt)
    let mut score = 0.0
    let mut iteration = 0

    while iteration < self.max_iterations
      let evaluation = evaluator.evaluate(output)
      score = evaluation.score

      if score >= self.quality_threshold
        break
      end

      output = generator.improve(output, feedback: evaluation.feedback)
      iteration += 1
    end

    return output
  end
end
```

#### 4.1.5 Swarm (Dynamic Routing)

```lumen
orchestration HelpDesk
  agents: [BillingAgent, TechSupport, AccountManager, Escalation]
  router: TriageAgent

  cell handle(request: String) -> Response / {llm, trace, approve}
    let mut current_agent = router.classify(request)
    let mut context = Context(request: request, history: [])

    loop
      let result = current_agent.respond(context)

      match result
        Response.Final(answer) ->
          return answer

        Response.Handoff(target, reason) ->
          context = context.with_handoff(from: current_agent, to: target, reason: reason)
          current_agent = self.agents.find(fn(a) => a.name == target)
            ?? halt("Unknown agent: {target}")

        Response.Escalate(reason) ->
          approve "Escalation requested: {reason}"
          current_agent = Escalation
      end
    end
  end
end
```

### 4.2 Orchestration Combinators

For ad-hoc composition without defining named orchestrations:

```lumen
# Run agents in sequence, threading output to input
let result = text
  |> agent Extractor.extract
  |> agent Validator.check
  |> agent Enricher.augment

# Run agents in parallel, collect all results
let (research, analysis) = await parallel
  agent Researcher.search(topic)
  agent Analyst.find_trends(topic)
end

# Race: first agent to respond wins
let fastest = await race
  agent FastModel.respond(query)
  agent SlowModel.respond(query)
end

# Vote: majority rules
let consensus = await vote(threshold: 0.66)
  agent Agent1.classify(input)
  agent Agent2.classify(input)
  agent Agent3.classify(input)
end
```

### 4.3 Orchestration Traces

Multi-agent orchestrations produce enriched traces:

| Event | Description |
|-------|-------------|
| `orchestration_start` | Orchestration begins |
| `orchestration_end` | Orchestration completes |
| `agent_enter` | Control passes to an agent |
| `agent_exit` | Agent returns control |
| `handoff` | Agent-to-agent transfer |
| `escalation` | Human escalation triggered |
| `stage_start` | Pipeline stage begins |
| `stage_end` | Pipeline stage completes |
| `round_start` | Debate/refinement round begins |
| `round_end` | Round completes with score |
| `vote_cast` | Agent casts vote |
| `vote_result` | Voting concludes |

---

# Part III: State Machines

---

## 5. First-Class State Machines

State machines model workflows with explicit states, transitions, and guards. LangGraph won the agent framework race specifically because of this abstraction. Lumen makes it a language construct.

### 5.1 Declaration

```lumen
machine TicketHandler
  ## Initial state
  initial: Triage

  ## State definitions
  state Triage
    on_enter(ticket: Ticket) / {llm, trace}
      let classification = Classifier.classify(ticket)
      match classification.severity
        Severity.Critical -> transition Escalate(ticket, classification)
        Severity.Normal   -> transition Assign(ticket, classification)
        Severity.Spam     -> transition Close(ticket, reason: "spam")
      end
    end
  end

  state Assign
    on_enter(ticket: Ticket, classification: Classification) / {llm, trace}
      let agent = Router.find_best_agent(classification)
      transition InProgress(ticket, agent)
    end
  end

  state Escalate
    on_enter(ticket: Ticket, classification: Classification) / {approve, trace}
      approve "Critical ticket requires human review: {ticket.summary}"
        context: {ticket: ticket, classification: classification}
        timeout: 1h
        on_timeout: transition Assign(ticket, classification)
      end

      transition Assign(ticket, classification)
    end
  end

  state InProgress
    timeout: 24h

    on_enter(ticket: Ticket, agent: Agent) / {llm, trace}
      let resolution = agent.work(ticket)
      match resolution
        Resolution.Solved(answer) -> transition Resolved(ticket, answer)
        Resolution.NeedInfo(question) -> transition WaitingOnCustomer(ticket, question)
        Resolution.Stuck(reason) -> transition Escalate(ticket, Classification(severity: Severity.Critical))
      end
    end

    on_timeout(ticket: Ticket, agent: Agent) / {trace}
      transition Escalate(ticket, Classification(severity: Severity.Critical))
    end
  end

  state WaitingOnCustomer
    timeout: 72h

    on_enter(ticket: Ticket, question: String) / {emit}
      emit("Sent follow-up to customer: {question}")
    end

    on_event CustomerReply(ticket: Ticket, reply: String) / {trace}
      transition InProgress(ticket, Router.find_best_agent(ticket))
    end

    on_timeout(ticket: Ticket, question: String) / {trace}
      transition Close(ticket, reason: "no_response")
    end
  end

  state Resolved
    on_enter(ticket: Ticket, answer: String) / {emit, trace}
      emit("Ticket resolved: {answer}")
      transition Close(ticket, reason: "resolved")
    end
  end

  state Close
    terminal: true
    on_enter(ticket: Ticket, reason: String) / {trace}
      emit("Ticket closed: {reason}")
    end
  end
end
```

### 5.2 Running a State Machine

```lumen
let handler = TicketHandler()
let final_state = handler.run(ticket)

# Or step through manually
let machine = TicketHandler()
machine.start(ticket)

while not machine.is_terminal()
  let current = machine.current_state()
  log("Currently in: {current.name}")

  machine.step()  # advance one transition
end
```

### 5.3 State Machine Properties

The compiler verifies:

- **Reachability:** Every state is reachable from the initial state
- **Terminal coverage:** At least one terminal state exists
- **Transition typing:** Every `transition` call passes arguments matching the target state's `on_enter` signature
- **Exhaustive timeouts:** States with timeouts have `on_timeout` handlers
- **No orphan events:** Every `on_event` handler corresponds to a declared event type
- **Effect consistency:** The machine's overall effect row is the union of all state effects

### 5.4 State Machine Traces

State machines produce specialized trace events:

```json
{
  "kind": "state_transition",
  "from_state": "Triage",
  "to_state": "Assign",
  "trigger": "classification.severity == Normal",
  "data_hash": "sha256:...",
  "timestamp": "..."
}
```

This enables **visual replay** — a debugger can render the state machine diagram and animate the actual execution path.

### 5.5 Checkpointing

State machines support checkpointing for long-running workflows:

```lumen
machine LongWorkflow
  @checkpoint_on_transition  # save state on every transition

  state Processing
    on_enter(data: Data) / {llm, trace}
      # If the process crashes here, it resumes from the last checkpoint
      let result = expensive_operation(data)
      transition Complete(result)
    end
  end
end

# Resume from checkpoint
let machine = LongWorkflow.resume_from(".lumen/checkpoints/abc123.json")
```

---

# Part IV: Memory System

---

## 6. Memory Abstractions

Production agents need persistent, queryable memory. Lumen provides four memory kinds as language-level abstractions.

### 6.1 Memory Kinds

| Kind | Storage | Query Model | Lifetime |
|------|---------|-------------|----------|
| `short_term` | In-memory ring buffer | Window-based (last N items) | Session |
| `episodic` | Persistent store | Similarity search (embeddings) | Cross-session |
| `entity` | Structured store | Key-value with schema | Cross-session |
| `procedural` | Definition store | Name/hash lookup | Permanent |

### 6.2 Short-Term Memory

Conversation context and recent interactions:

```lumen
memory ConversationBuffer: short_term
  window: 20           # keep last 20 messages
  format: message       # store as Message records
end

memory WorkingMemory: short_term
  window: 100
  format: json
  eviction: lru         # least recently used
end
```

Usage:

```lumen
agent ChatBot
  memory: [ConversationBuffer]

  cell respond(user_input: String) -> String / {llm, state}
    # Memory is automatically available as context
    let context = self.memory.ConversationBuffer.recent(10)

    let response = Chat(
      role system: self.role,
      role user: user_input,
      context: context
    )

    # Automatically stored
    self.memory.ConversationBuffer.append(
      Message(role: "user", content: user_input),
      Message(role: "assistant", content: response)
    )

    return response
  end
end
```

### 6.3 Episodic Memory

Long-term memory retrieved by semantic similarity:

```lumen
memory KnowledgeBase: episodic
  embedding_model: "text-embedding-3-small"
  store: "local"              # or "postgres", "pinecone", etc.
  max_results: 5
  similarity_threshold: 0.7
end

# Store
self.memory.KnowledgeBase.remember(
  content: "The user prefers formal communication style",
  metadata: {source: "conversation", date: timestamp()}
)

# Retrieve
let relevant = self.memory.KnowledgeBase.recall(
  query: "How should I address this user?",
  max_results: 3
)
```

### 6.4 Entity Memory

Structured facts about entities in the world:

```lumen
record UserProfile
  name: String
  preferences: map[String, String]
  interaction_count: Int
  last_seen: Int
end

memory UserFacts: entity
  schema: UserProfile
  key: String                  # entity ID
  store: "local"
end

# Store
self.memory.UserFacts.upsert("user_123", UserProfile(
  name: "Alice",
  preferences: {"tone": "formal", "length": "concise"},
  interaction_count: 42,
  last_seen: timestamp()
))

# Retrieve
let profile = self.memory.UserFacts.get("user_123")

# Query
let active_users = self.memory.UserFacts.query(
  fn(u) => u.last_seen > timestamp() - duration(days: 7).total_ms()
)
```

### 6.5 Procedural Memory

Learned workflows and strategies. Stored as Lumen definitions (content-addressed):

```lumen
memory Procedures: procedural
  store: "local"
end

# An agent can learn new procedures from experience
cell learn_procedure(name: String, steps: list[String]) / {state}
  let procedure = compile_steps(steps)  # creates a cell at runtime
  self.memory.Procedures.store(name, procedure)
end

# And recall them later
cell execute_learned(name: String, input: Json) -> Json / {state}
  let procedure = self.memory.Procedures.recall(name)?
  return procedure.execute(input)
end
```

### 6.6 Memory in Traces

All memory operations produce trace events:

```json
{
  "kind": "memory_write",
  "memory_name": "KnowledgeBase",
  "memory_kind": "episodic",
  "content_hash": "sha256:...",
  "metadata": {...}
}
```

```json
{
  "kind": "memory_read",
  "memory_name": "KnowledgeBase",
  "query_hash": "sha256:...",
  "results_count": 3,
  "top_similarity": 0.89
}
```

### 6.7 Memory Scoping

Memory can be scoped to different levels:

```lumen
# Agent-private memory (only this agent can access)
memory PrivateNotes: short_term
  scope: agent
end

# Shared across orchestration (all agents in a team can access)
memory SharedContext: short_term
  scope: orchestration
end

# Global (persistent across all sessions)
memory GlobalKnowledge: episodic
  scope: global
end
```

---

# Part V: Human-in-the-Loop

---

## 7. Approval Primitives

### 7.1 The `approve` Block

```lumen
cell delete_records(ids: list[String]) -> result[Int, String] / {approve, fs, trace}
  let records = fetch_records(ids)

  # Execution pauses here until a human approves
  approve "Delete {records.length} records?"
    context: records                    # data shown to the reviewer
    reviewers: ["admin@example.com"]    # who can approve
    timeout: 24h                        # max wait time
    on_approve:
      continue                          # resume execution
    on_reject(reason):
      return err("Rejected: {reason}")
    on_timeout:
      return err("Approval timed out")
  end

  let deleted = perform_deletion(ids)
  return ok(deleted)
end
```

### 7.2 The `checkpoint` Block

Save execution state for resumption:

```lumen
cell long_running_analysis(data: list[Item]) -> Report / {llm, trace}
  let mut results: list[AnalysisResult] = []

  for (i, item) in data.enumerate()
    let result = analyze_single(item)
    results = results ++ [result]

    # Save progress every 10 items
    if i % 10 == 0
      checkpoint "analysis_progress"
        state: {completed: i, results: results}
        resume_from: "continue_loop"
      end
    end
  end

  @label("continue_loop")
  return compile_report(results)
end

# Resume from checkpoint
let report = resume("analysis_progress", checkpoint_id: "abc123")
```

### 7.3 The `escalate` Primitive

Transfer control from an agent to a human:

```lumen
cell handle_complaint(complaint: String) -> Resolution / {llm, approve, trace}
  let severity = assess_severity(complaint)

  if severity.score > 0.8
    escalate "High-severity complaint requires human handling"
      context: {complaint: complaint, severity: severity}
      channel: "support-escalations"
      on_human_response(response: String):
        return Resolution(handled_by: "human", response: response)
      on_timeout(48h):
        return Resolution(handled_by: "system", response: auto_response(complaint))
    end
  end

  return auto_resolve(complaint)
end
```

### 7.4 The `confirm` Expression

Lightweight inline confirmation:

```lumen
cell send_email(to: String, body: String) -> Bool / {approve, email}
  let should_send = confirm "Send email to {to}?"
  if should_send
    EmailTool.send(to: to, body: body)
    return true
  end
  return false
end
```

### 7.5 HITL Trace Events

| Event | Description |
|-------|-------------|
| `approval_requested` | Approval prompt created |
| `approval_granted` | Human approved |
| `approval_denied` | Human rejected (with reason) |
| `approval_timeout` | Timeout expired |
| `checkpoint_saved` | Execution state persisted |
| `checkpoint_resumed` | Execution resumed from checkpoint |
| `escalation_created` | Escalation sent to human queue |
| `escalation_resolved` | Human responded to escalation |
| `confirmation_requested` | Inline confirmation prompted |
| `confirmation_response` | Human responded |

---

# Part VI: Guardrails

---

## 8. Guardrail Declarations

### 8.1 Syntax

```lumen
guardrail <Name>
  [description: <string>]
  [on_input: <handler>]
  [on_output: <handler>]
  [on_tool_call: <handler>]
  [on_violation: <action>]
end
```

### 8.2 Input/Output Guardrails

```lumen
guardrail PIIProtection
  description: "Prevents personally identifiable information from leaking"

  on_input(data: String) -> String / {pure}
    return redact_patterns(data, [
      r"\b\d{3}-\d{2}-\d{4}\b",           # SSN
      r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b",  # credit card
      r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b"  # email
    ])
  end

  on_output(data: String) -> result[String, GuardrailViolation] / {pure}
    let pii_found = detect_pii(data)
    if pii_found.length > 0
      return err(GuardrailViolation(
        guardrail: "PIIProtection",
        details: "PII detected in output: {pii_found.map(fn(p) => p.type).join(', ')}",
        locations: pii_found
      ))
    end
    return ok(data)
  end

  on_violation: redact_and_retry(max: 2)
end
```

### 8.3 Content Policy Guardrails

```lumen
guardrail ContentSafety
  description: "Ensures all output complies with content policy"

  on_output(data: String) -> result[String, GuardrailViolation] / {llm}
    let check = Chat(
      role system:
        You are a content safety classifier. Evaluate the following text
        for harmful, biased, or inappropriate content. Return JSON:
        {"safe": true/false, "reason": "...", "category": "..."}
      end,
      role user: data
    ) expect schema SafetyCheck

    if not check.safe
      return err(GuardrailViolation(
        guardrail: "ContentSafety",
        details: check.reason,
        category: check.category
      ))
    end
    return ok(data)
  end

  on_violation: halt("Content policy violation: {violation.details}")
end
```

### 8.4 Tool Call Guardrails

```lumen
guardrail BudgetGuard
  description: "Prevents exceeding LLM cost budget"

  state:
    let mut total_cost: Float = 0.0
    let budget: Float = 50.0  # USD
  end

  on_tool_call(tool_id: String, args: Json) -> result[Json, GuardrailViolation] / {state}
    if tool_id.starts_with("llm.")
      let estimated_cost = estimate_cost(args)
      if self.state.total_cost + estimated_cost > self.state.budget
        return err(GuardrailViolation(
          guardrail: "BudgetGuard",
          details: "Budget exceeded: ${self.state.total_cost} + ${estimated_cost} > ${self.state.budget}"
        ))
      end
    end
    return ok(args)
  end

  on_tool_result(tool_id: String, result: Json) / {state}
    if tool_id.starts_with("llm.")
      let actual_cost = result["usage"]["cost"].as_float().unwrap_or(0.0)
      self.state.total_cost += actual_cost
    end
  end

  on_violation: halt("LLM budget exceeded")
end
```

### 8.5 Composing Guardrails

Guardrails compose as an ordered pipeline:

```lumen
agent CustomerAgent
  guardrails: [
    PIIProtection,        # runs first on input, last on output
    ContentSafety,        # runs second on input, second-to-last on output
    BudgetGuard           # runs on every tool call
  ]
end
```

Input guardrails execute top-to-bottom. Output guardrails execute bottom-to-top (onion model). If any guardrail returns an error, the violation handler fires.

### 8.6 Guardrail Traces

```json
{
  "kind": "guardrail_check",
  "guardrail": "PIIProtection",
  "phase": "output",
  "result": "pass",
  "latency_ms": 2
}
```

```json
{
  "kind": "guardrail_violation",
  "guardrail": "ContentSafety",
  "phase": "output",
  "details": "Potentially biased language detected",
  "action": "halt"
}
```

---

# Part VII: Evaluation Framework

---

## 9. Built-In Evals

### 9.1 Eval Declaration

```lumen
eval InvoiceExtractionAccuracy
  description: "Measures invoice extraction quality"
  dataset: "test/invoices.jsonl"
  agent: InvoiceExtractor

  record TestCase
    input: String
    expected: Invoice
  end

  cell evaluate(case: TestCase) -> EvalResult / {llm, trace}
    let actual = self.agent.extract(case.input)

    match actual
      ok(invoice) ->
        return EvalResult(
          passed: invoice == case.expected,
          metrics: {
            "exact_match": if invoice == case.expected then 1.0 else 0.0,
            "field_accuracy": field_accuracy(invoice, case.expected),
            "total_accuracy": field_accuracy(invoice, case.expected),
            "latency_ms": trace_ref().last_event().latency_ms
          }
        )
      err(e) ->
        return EvalResult(
          passed: false,
          metrics: {
            "exact_match": 0.0,
            "field_accuracy": 0.0,
            "error": e.message
          }
        )
    end
  end

  thresholds:
    exact_match >= 0.90
    field_accuracy >= 0.95
    latency_ms.p99 <= 5000
  end
end
```

### 9.2 Running Evals

```bash
lumen eval InvoiceExtractionAccuracy
lumen eval InvoiceExtractionAccuracy --parallel --workers 4
lumen eval InvoiceExtractionAccuracy --subset 0.1  # 10% sample
lumen eval --all                                      # run all evals
lumen eval --compare baseline.json                    # diff against baseline
```

### 9.3 Eval Output

```json
{
  "eval": "InvoiceExtractionAccuracy",
  "timestamp": "2026-02-12T...",
  "dataset_size": 100,
  "duration_ms": 45000,
  "results": {
    "exact_match": {"mean": 0.92, "std": 0.08, "p50": 0.95, "p99": 0.70},
    "field_accuracy": {"mean": 0.97, "std": 0.03, "p50": 0.98, "p99": 0.88},
    "latency_ms": {"mean": 1200, "p50": 980, "p95": 2400, "p99": 4100}
  },
  "thresholds": {
    "exact_match": {"target": 0.90, "actual": 0.92, "pass": true},
    "field_accuracy": {"target": 0.95, "actual": 0.97, "pass": true},
    "latency_ms.p99": {"target": 5000, "actual": 4100, "pass": true}
  },
  "overall": "PASS"
}
```

### 9.4 LLM-as-Judge Evals

```lumen
eval ResponseQuality
  description: "Judges response quality using an LLM"
  dataset: "test/conversations.jsonl"
  agent: CustomerSupport

  cell evaluate(case: TestCase) -> EvalResult / {llm}
    let actual_response = self.agent.respond(case.input)

    # Use a different (stronger) model as judge
    let judgment = Chat(
      model: "claude-sonnet-4-20250514",
      role system:
        You are evaluating AI assistant responses. Score 1-5 on:
        - Helpfulness: Does it address the user's need?
        - Accuracy: Is the information correct?
        - Safety: Is it free from harmful content?
        - Tone: Is the tone appropriate?
        Return JSON: {"helpfulness": N, "accuracy": N, "safety": N, "tone": N}
      end,
      role user:
        User query: {case.input}
        Assistant response: {actual_response}
        Expected behavior: {case.expected_behavior}
      end
    ) expect schema JudgmentScore

    return EvalResult(
      passed: judgment.helpfulness >= 4 and judgment.safety >= 4,
      metrics: {
        "helpfulness": judgment.helpfulness.to_float(),
        "accuracy": judgment.accuracy.to_float(),
        "safety": judgment.safety.to_float(),
        "tone": judgment.tone.to_float()
      }
    )
  end

  thresholds:
    helpfulness.mean >= 4.0
    safety.mean >= 4.5
  end
end
```

### 9.5 Regression Testing with Evals

```lumen
eval RegressionSuite
  description: "Ensures no regression from baseline"
  baseline: ".lumen/eval/baseline.json"

  cell evaluate(case: TestCase) -> EvalResult / {llm}
    let result = self.agent.run(case.input)

    let baseline_result = self.baseline.get(case.id)

    return EvalResult(
      passed: result.quality >= baseline_result.quality * 0.95,  # 5% tolerance
      metrics: {
        "quality": result.quality,
        "baseline_quality": baseline_result.quality,
        "delta": result.quality - baseline_result.quality
      }
    )
  end

  thresholds:
    delta.mean >= -0.02  # no more than 2% regression on average
  end
end
```

### 9.6 Eval CI Integration

```bash
# In CI pipeline
lumen eval --all --output results.json --fail-on-threshold
# Exit code 0 if all thresholds pass, 1 if any fail

# Snapshot baseline
lumen eval --all --save-baseline .lumen/eval/baseline.json
```

---

# Part VIII: Schema Evolution

---

## 10. Versioned Schemas

### 10.1 Version Annotations

```lumen
@version(1)
record Invoice
  id: String
  total: Float
end

@version(2)
record Invoice
  id: String
  total: Float
  currency: String = "USD"

  migrate from v1(old: Invoice@v1) -> Invoice@v2
    return Invoice(
      id: old.id,
      total: old.total,
      currency: "USD"
    )
  end
end

@version(3)
record Invoice
  id: String
  subtotal: Float
  tax: Float
  total: Float
  currency: String

  migrate from v2(old: Invoice@v2) -> Invoice@v3
    return Invoice(
      id: old.id,
      subtotal: old.total,
      tax: 0.0,
      total: old.total,
      currency: old.currency
    )
  end

  migrate from v1(old: Invoice@v1) -> Invoice@v3
    let v2 = Invoice@v2.migrate_from_v1(old)
    return Invoice@v3.migrate_from_v2(v2)
  end
end
```

### 10.2 Cache Migration

When a schema version changes, cached tool outputs validated against the old schema can be automatically migrated:

```lumen
@cache_migration
cell migrate_cache(tool_id: String, old_version: Int, new_version: Int) / {cache, trace}
  let entries = cache.list(tool: tool_id)

  for entry in entries
    let old_data = entry.value
    let migrated = migrate_schema(old_data, from: old_version, to: new_version)

    match migrated
      ok(new_data) ->
        cache.update(entry.key, new_data)
      err(e) ->
        cache.invalidate(entry.key)
        emit("Cache entry {entry.key} invalidated: {e.message}")
    end
  end
end
```

### 10.3 Trace Compatibility

Traces reference the schema version that was active when they were created:

```json
{
  "kind": "schema_validate",
  "schema": "Invoice",
  "schema_version": 2,
  "schema_hash": "sha256:...",
  ...
}
```

When replaying old traces, the runtime automatically loads the correct schema version. If the current code uses a newer schema, the migration chain is applied.

### 10.4 Breaking vs Non-Breaking Changes

The compiler classifies schema changes:

| Change | Classification |
|--------|---------------|
| Add field with default value | Non-breaking |
| Add required field without default | Breaking (migration required) |
| Remove field | Breaking (migration required) |
| Rename field | Breaking (migration required) |
| Change field type | Breaking (migration required) |
| Add constraint | Potentially breaking |
| Remove constraint | Non-breaking |
| Change default value | Non-breaking |

Breaking changes without a `migrate` block are compile errors.

---

# Part IX: Formal Semantics

---

## 11. Operational Semantics

For Lumen's claims of "provable" to be real, we provide formal small-step operational semantics for the core calculus.

### 11.1 The Core Calculus: lambda_Lumen

We define a minimal calculus that captures Lumen's essential features:

**Syntax:**

```
Types:
  t ::= B                          base types (String, Int, Float, Bool)
      | t1 -> t2 / e               effectful function (effect row e)
      | {l1: t1, ..., ln: tn}      record
      | <l1: t1 | ... | ln: tn>    variant/union
      | list[t]                     list
      | result[t1, t2]             tagged result

Effects:
  e ::= {}                         empty (pure)
      | {e1, ..., en}              concrete effect set
      | {e1, ..., en | u}          open effect row (row variable u)

  e ::= http | llm | fs | trace    built-in effect labels
      | mcp | state | approve
      | E                           user-defined effect label

Expressions:
  expr ::= x                       variable
         | c                        constant/literal
         | \x: t. expr              abstraction
         | expr1 expr2              application
         | let x = expr1 in expr2   binding
         | {l1 = expr1, ...}        record literal
         | expr.l                    field access
         | match expr { pi -> expri } pattern match
         | tool[t](expr)            tool call (effect: t's kind)
         | validate[t](expr)        schema validation
         | handle e { hi } in expr  effect handler

Values:
  v ::= c                          constants
       | \x: t. expr               closures
       | {l1 = v1, ...}            record values
       | ok(v) | err(v)            result values

Contexts:
  E ::= _                          hole
      | E expr                     application (left)
      | v E                        application (right)
      | let x = E in expr          let binding
      | {l1 = v1, ..., li = E, ...}  record
      | E.l                        field access
      | tool[t](E)                 tool call argument
      | validate[t](E)             validation argument
```

### 11.2 Typing Rules

**Pure function:**

```
  G, x: t1 |- expr : t2 / e
  ─────────────────────────────
  G |- \x: t1. expr : t1 -> t2 / e
```

**Application:**

```
  G |- expr1 : t1 -> t2 / e1    G |- expr2 : t1 / e2
  ──────────────────────────────────────────────────────
  G |- expr1 expr2 : t2 / e1 U e2
```

**Tool call:**

```
  G |- expr : t_in / e    tool t : t_in -> t_out    effect_of(t) = e
  ─────────────────────────────────────────────────────────────────
  G |- tool[t](expr) : t_out / e U {e}
```

**Schema validation:**

```
  G |- expr : Json / e    t is a schema type
  ──────────────────────────────────────────
  G |- validate[t](expr) : result[t, ValidationError] / e
```

**Let binding:**

```
  G |- expr1 : t1 / e1    G, x: t1 |- expr2 : t2 / e2
  ──────────────────────────────────────────────────────
  G |- let x = expr1 in expr2 : t2 / e1 U e2
```

**Effect subsumption:**

```
  G |- expr : t / e1    e1 <= e2
  ────────────────────────────────
  G |- expr : t / e2
```

**Effect handler:**

```
  G |- expr : t / e U {e}    for each op in e: G |- h_op handles op
  ─────────────────────────────────────────────────────────────────
  G |- handle {e} { h1, ..., hn } in expr : t / e
```

The handler removes effect `e` from the row — the handled expression has `e` in its effects, but the overall `handle` block does not.

### 11.3 Small-Step Reduction Rules

**Beta-reduction (function application):**

```
  (\x: t. expr) v  -->  expr[v/x]
```

**Let reduction:**

```
  let x = v in expr  -->  expr[v/x]
```

**Field access:**

```
  {l1 = v1, ..., ln = vn}.li  -->  vi
```

**Tool call (effect step):**

```
                               tool t invoked with v
  tool[t](v)  -->  <tool_call, t, v, _>    (suspends, awaits result)

                               tool returns r
  <tool_call, t, v, _>  -->  r              (resumes with result)
```

The `<tool_call, t, v, _>` is a **suspension** — the computation pauses while the external tool executes. This is the formal model of Lumen's effect boundary. The trace system hooks into this transition.

**Validation:**

```
  validate[t](v) where v matches t   -->  ok(v : t)
  validate[t](v) where v !matches t  -->  err(ValidationError(...))
```

**Pattern matching:**

```
  match v { p1 -> e1 | ... | pn -> en }  -->  ei[s]
    where pi is the first pattern matching v with substitution s
```

**Effect handling:**

```
  handle {e} { op(x, k) -> h } in E[perform e.op(v)]
    -->  h[v/x, (\y. handle {e} { ... } in E[y])/k]
```

This is the standard algebraic effect handler semantics: the handler captures the continuation `k` and can resume it, discard it, or invoke it multiple times.

### 11.4 Trace Semantics

We define trace generation as a labeled transition system. Each reduction step may produce a trace event:

```
  expr, s, T  -->_t  expr', s', T . t
```

Where `s` is the store (heap), `T` is the trace (sequence of events), and `t` is the new trace event (or epsilon for no event).

**Trace-producing rules:**

```
  tool[t](v), s, T  -->_t  r, s', T . <tool_call, t, hash(v), hash(r), latency>
```

```
  validate[t](v), s, T  -->_t  r, s, T . <schema_validate, t, hash(v), result>
```

```
  cell f enters, s, T  -->_t  ..., s, T . <cell_enter, f, hash(args)>
  cell f returns v, s, T  -->_t  v, s, T . <cell_exit, f, hash(v)>
```

### 11.5 Soundness Theorems

**Theorem 1 (Progress):** If `G |- expr : t / e` and `expr` is not a value or suspension, then there exists `expr'` such that `expr --> expr'`.

*Every well-typed, non-value, non-suspended expression can take a step.*

**Theorem 2 (Preservation):** If `G |- expr : t / e` and `expr --> expr'`, then `G |- expr' : t / e'` where `e' <= e`.

*Reduction preserves types and can only reduce effects (never add new ones).*

**Theorem 3 (Effect Safety):** If `G |- expr : t / {}`, then `expr` reduces to a value without any tool calls, I/O, or suspensions.

*Pure expressions are truly pure.*

**Theorem 4 (Trace Determinism):** If `expr, s, T0 -->* v, s1, T1` and all tool calls return identical results, then for any second execution `expr, s, T0 -->* v', s2, T2`, we have `v = v'` and `hash_chain(T1) = hash_chain(T2)`.

*Given the same tool outputs, execution is deterministic and traces are identical.*

**Theorem 5 (Capability Confinement):** If a cell `f` has effect row `e` and `http not-in e`, then no execution path through `f` can invoke an HTTP tool.

*The effect system provides confinement — capabilities are enforced statically.*

### 11.6 Denotational Sketch

For the mathematically inclined, Lumen's denotational semantics maps to:

- **Pure expressions:** Total functions between value domains
- **Effectful expressions:** Free monad over the effect signature (the "freer monad" construction)
- **Tool calls:** Operations in the free monad
- **Effect handlers:** Folds over the free monad
- **Traces:** A writer monad layered over the effect monad

The composition `Writer[Trace] . Free[Effects]` gives the correct semantics for traced, effectful computations.

---

# Part X: Advanced Pattern Matching

---

## 12. Extended Patterns

### 12.1 Active Patterns

User-defined pattern decomposition:

```lumen
@active_pattern
cell Even(n: Int) -> Bool = n % 2 == 0

@active_pattern
cell InRange(n: Int, lo: Int, hi: Int) -> Bool = n >= lo and n <= hi

@active_pattern
cell EmailParts(s: String) -> (String, String) | Null
  let parts = s.split("@")
  if parts.length == 2
    return (parts[0], parts[1])
  end
  return null
end

match value
  Even() -> "even number"
  _ -> "odd number"
end

match email
  EmailParts(local, domain) -> "user: {local}, domain: {domain}"
  _ -> "not an email"
end
```

### 12.2 View Patterns

Transform the scrutinee before matching:

```lumen
match users
  (sorted_by(.age) -> [youngest, .._]) -> "youngest: {youngest.name}"
end

match response.body
  (json_parse -> ok({"status": "success", "data": data})) ->
    process(data)
  (json_parse -> err(e)) ->
    halt("Invalid JSON: {e}")
end
```

### 12.3 Pattern Synonyms

Named patterns for readability:

```lumen
pattern HttpOk(body) = Response(status: 200, body: body, ..)
pattern HttpError(status, msg) = Response(status: status, body: msg, ..) if status >= 400

match response
  HttpOk(body) -> process(body)
  HttpError(404, _) -> not_found()
  HttpError(status, msg) -> halt("HTTP {status}: {msg}")
end
```

---

# Part XI: Observability

---

## 13. Structured Logging and Metrics

### 13.1 The `observe` Block

```lumen
cell process_batch(items: list[Item]) -> list[Result] / {llm, trace}
  observe "batch_processing"
    tags: {batch_size: items.length}
    metrics:
      counter items_processed
      histogram processing_time_ms
      gauge active_items
    end
  in
    let mut results: list[Result] = []
    for item in items
      let start = timestamp()
      let result = process_single(item)
      results = results ++ [result]

      metrics.items_processed.increment()
      metrics.processing_time_ms.record(timestamp() - start)
      metrics.active_items.set(items.length - results.length)
    end
    return results
  end
end
```

### 13.2 Structured Trace Annotations

```lumen
cell important_operation() / {trace}
  trace.annotate("business_context", {
    "customer_id": customer.id,
    "operation": "account_update",
    "risk_level": "high"
  })

  trace.span("validation") in
    validate_input()
  end

  trace.span("execution") in
    perform_update()
  end
end
```

### 13.3 OpenTelemetry Export

```lumen
@trace_export("otlp")
@trace_endpoint("http://localhost:4317")

# Lumen traces automatically export to OpenTelemetry-compatible backends
# (Jaeger, Zipkin, Datadog, Grafana Tempo, etc.)
```

---

# Part XII: Distributed Execution (Preview)

---

## 14. Location-Transparent Cells

### 14.1 Remote Execution

```lumen
@remote("gpu-cluster")
cell embed_documents(docs: list[String]) -> list[Embedding] / {llm}
  return EmbeddingModel.encode(docs)
end

@remote("us-east-1")
cell fetch_us_data(query: String) -> list[Record] / {database}
  return DbQuery(sql: query)
end
```

The compiler generates serialization/deserialization code automatically (all Lumen values are serializable by construction). The runtime handles:

1. Serializing arguments (using canonical JSON)
2. Transferring the cell definition by hash (if not present on the remote)
3. Executing remotely
4. Returning results
5. Merging remote trace events into the local trace

### 14.2 Location Policies

```lumen
@location_policy
  prefer: "local"
  fallback: ["us-east-1", "eu-west-1"]
  timeout_ms: 5000
  retry_on_failure: true
end
```

### 14.3 Distributed State Machines

```lumen
machine DistributedOrder
  @replicated(factor: 3)

  state Pending
    # ...
  end

  state Processing
    @location("warehouse-{region}")
    # ...
  end

  state Shipped
    # ...
  end
end
```

---

# Part XIII: Appendices

---

## A. Complete V2 Effect Hierarchy

```
total (pure, {})
+-- exn (may raise exceptions)
+-- diverge (may not terminate)
+-- state (reads/writes mutable state)
+-- trace (emits trace events)
+-- emit (produces user output)
+-- time (depends on wall clock)
+-- random (non-deterministic)
+-- cache (reads/writes cache)
+-- approve (human-in-the-loop)
+-- http (network I/O)
+-- llm (language model calls)
+-- fs (filesystem I/O)
+-- mcp (MCP server calls)
+-- database (DB operations)
+-- email (email operations)
+-- <user-defined> (custom effects)

Composed effects:
  io = {http, fs, state, trace, emit, time}
  ai = {llm, http, trace, cache}
  agent = {llm, http, trace, cache, state, approve, emit}
```

## B. V2 Keyword Additions

The following keywords are added in V2:

```
agent       approve     bind        checkpoint  collaborators
confirm     effect      escalate    eval        guardrail
handle      machine     memory      observe     orchestration
pattern     perform     pipeline    remote      resume
scope       state       thresholds  transition  with
workflow    vote        race        yield
```

## C. Migration Guide: V1 to V2

| V1 Pattern | V2 Equivalent |
|------------|---------------|
| `@pure` annotation | `/ {}` effect row (compiler-verified) |
| Cell with tool calls | Effect row auto-inferred |
| Manual mock in tests | `handle` effect handler |
| Cell with system prompt | `agent` declaration |
| Manual parallel coordination | `orchestration` declaration |
| No state tracking | `machine` declaration |
| No memory | `memory` declarations |
| No approval flow | `approve` / `escalate` blocks |
| No guardrails | `guardrail` declarations |
| Manual eval scripts | `eval` declarations |

V1 programs are valid V2 programs. Effect rows are inferred if not annotated. All V2 features are opt-in.

## D. Research References

| Feature | Based On | Reference |
|---------|----------|-----------|
| Typed effect rows | Koka | Leijen, "Programming with Row Polymorphic Effect Types" (2014) |
| Effect handlers | Eff, OCaml 5 | Plotkin & Pretnar, "Handlers of Algebraic Effects" (2009) |
| Content-addressed definitions | Unison | Chiusano & Bjarnason, Unison language (2013-2025) |
| Object-capability model | E, Pony | Miller, "Robust Composition" (2006) |
| State machines | Statecharts | Harel, "Statecharts: A Visual Formalism" (1987) |
| Multi-agent orchestration | LangGraph, CrewAI | Industry convergence (2023-2025) |
| Guardrails | NeMo Guardrails | NVIDIA, NeMo Guardrails (2023) |
| Algebraic effects compilation | Koka, Effekt | Xie et al., "Efficient Compilation of Algebraic Effect Handlers" (2020) |
| Content-addressed caching | Nix, Unison | Dolstra, "The Purely Functional Software Deployment Model" (2006) |
| Formal verification | Liquid Haskell | Vazou et al., "Refinement Types for Haskell" (2014) |
| Agent patterns | Anthropic research | "Building Effective Agents" (2024) |
| Orchestration patterns | Microsoft, AWS | "AI Agent Design Patterns" (2025) |

## E. Feature Phase Plan

| Feature | Phase | Blocks |
|---------|-------|--------|
| Effect row inference | V2.0 | Nothing - backward compatible |
| Effect annotations | V2.0 | Nothing |
| `agent` declaration | V2.0 | Nothing |
| `memory` system | V2.0 | Storage backend |
| `guardrail` declarations | V2.0 | Nothing |
| `approve` / HITL | V2.0 | Transport layer |
| `eval` framework | V2.0 | Nothing |
| Schema evolution | V2.0 | Migration tooling |
| Effect handlers | V2.1 | Continuation support in VM |
| Content-addressed definitions | V2.1 | Definition store |
| `machine` state machines | V2.1 | Checkpoint storage |
| `orchestration` patterns | V2.1 | Agent runtime |
| User-defined effects | V2.2 | Effect system maturity |
| Active patterns | V2.2 | Pattern compiler |
| Observability / OTel | V2.2 | Export layer |
| Formal semantics (verified) | V2.3 | Proof assistant formalization |
| Distributed execution | V3.0 | Network runtime |

---

*End of V2 Addendum*

*This document extends the Lumen V1 specification with typed effects, content-addressed definitions, first-class agents, multi-agent orchestration, state machines, memory, human-in-the-loop, guardrails, evaluations, schema evolution, and formal semantics. Together with V1, it defines the complete Lumen language — the first programming language designed from first principles for verifiable, composable, production-grade AI agent workflows.*
