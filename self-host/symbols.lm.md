# Symbol Table

The symbol table is built during the resolution pass and consumed by
typechecking, constraint validation, and lowering. It maps names to their
definitions across 14 categories, mirroring the Rust `SymbolTable` struct
in `resolve.rs`.

## Supporting Records

Information records for each symbol category.

```lumen
record ParamInfo(
  name: String,
  type_expr: String,
  variadic: Bool
)

record TypeInfo(
  kind: String,
  generic_params: list[String]
)

record CellInfo(
  params: list[ParamInfo],
  return_type: String?,
  effects: list[String],
  generic_params: list[String],
  must_use: Bool
)

record ToolInfo(
  tool_path: String,
  mcp_url: String?
)

record AgentInfo(
  name: String,
  methods: list[String]
)

record MachineStateInfo(
  name: String,
  params: list[ParamInfo],
  terminal: Bool,
  has_guard: Bool,
  transition_to: String?,
  transition_arg_count: Int
)

record ProcessInfo(
  kind: String,
  name: String,
  methods: list[String],
  pipeline_stages: list[String],
  machine_initial: String?,
  machine_states: list[MachineStateInfo]
)

record EffectInfo(
  name: String,
  operations: list[String]
)

record EffectBindInfo(
  effect_path: String,
  tool_alias: String
)

record HandlerInfo(
  name: String,
  handles: list[String]
)

record AddonInfo(
  kind: String,
  name: String?
)

record TraitInfo(
  name: String,
  parent_traits: list[String],
  methods: list[String]
)

record ImplInfo(
  trait_name: String?,
  target_type: String,
  methods: list[String]
)

record ConstInfo(
  name: String,
  type_expr: String?,
  has_value: Bool
)

record GrantPolicy(
  tool_alias: String,
  allowed_effects: list[String]?
)
```

## Symbol Table

The main symbol table containing maps for all 14 symbol categories.

```lumen
record SymbolTable(
  types: map[String, TypeInfo],
  cells: map[String, CellInfo],
  cell_policies: map[String, list[GrantPolicy]],
  tools: map[String, ToolInfo],
  agents: map[String, AgentInfo],
  processes: map[String, ProcessInfo],
  effects: map[String, EffectInfo],
  effect_binds: list[EffectBindInfo],
  handlers: map[String, HandlerInfo],
  addons: list[AddonInfo],
  type_aliases: map[String, String],
  traits: map[String, TraitInfo],
  impls: list[ImplInfo],
  consts: map[String, ConstInfo]
)
```

## Constructor

Create a new symbol table pre-populated with the 9 builtin types.

```lumen
cell new_symbol_table() -> SymbolTable
  let builtin_names = ["String", "Int", "Float", "Bool", "Bytes", "Json", "Null", "Self", "Any"]
  let types = {}
  for name in builtin_names
    let info = TypeInfo(kind: "builtin", generic_params: [])
    types = merge(types, {name: info})
  end
  SymbolTable(
    types: types,
    cells: {},
    cell_policies: {},
    tools: {},
    agents: {},
    processes: {},
    effects: {},
    effect_binds: [],
    handlers: {},
    addons: [],
    type_aliases: {},
    traits: {},
    impls: [],
    consts: {}
  )
end
```

## Scope Stack

Scoped symbol resolution. The resolver pushes a new scope when entering a
cell body, loop, or block, and pops it on exit. Name lookups walk from
the innermost scope outward.

```lumen
record Scope(
  variables: map[String, String],
  parent_index: Int?
)

record ScopeStack(
  scopes: list[Scope],
  current: Int
)

cell new_scope_stack() -> ScopeStack
  let root = Scope(variables: {}, parent_index: null)
  ScopeStack(scopes: [root], current: 0)
end

cell push_scope(stack: ScopeStack) -> ScopeStack
  let new_scope = Scope(variables: {}, parent_index: stack.current)
  let new_scopes = append(stack.scopes, new_scope)
  let new_index = length(new_scopes) - 1
  ScopeStack(scopes: new_scopes, current: new_index)
end

cell pop_scope(stack: ScopeStack) -> ScopeStack
  let scope = stack.scopes[stack.current]
  match scope.parent_index
    case null ->
      # Cannot pop root scope, return unchanged
      stack
    case parent ->
      ScopeStack(scopes: stack.scopes, current: parent)
  end
end

cell lookup_variable(stack: ScopeStack, name: String) -> String?
  let index = stack.current
  while index >= 0
    let scope = stack.scopes[index]
    if contains(scope.variables, name) then
      scope.variables[name]
    else
      match scope.parent_index
        case null ->
          return null
        case parent ->
          index = parent
      end
    end
  end
  null
end

cell define_variable(stack: ScopeStack, name: String, type_name: String) -> ScopeStack
  let scope = stack.scopes[stack.current]
  let new_vars = merge(scope.variables, {name: type_name})
  let new_scope = Scope(variables: new_vars, parent_index: scope.parent_index)
  let new_scopes = stack.scopes
  new_scopes[stack.current] = new_scope
  ScopeStack(scopes: new_scopes, current: stack.current)
end
```

## Import Helpers

Cells for importing symbols from an external module's symbol table.

```lumen
cell import_cell(table: SymbolTable, name: String, info: CellInfo) -> SymbolTable
  let new_cells = merge(table.cells, {name: info})
  SymbolTable(
    types: table.types,
    cells: new_cells,
    cell_policies: table.cell_policies,
    tools: table.tools,
    agents: table.agents,
    processes: table.processes,
    effects: table.effects,
    effect_binds: table.effect_binds,
    handlers: table.handlers,
    addons: table.addons,
    type_aliases: table.type_aliases,
    traits: table.traits,
    impls: table.impls,
    consts: table.consts
  )
end

cell import_type(table: SymbolTable, name: String, info: TypeInfo) -> SymbolTable
  let new_types = merge(table.types, {name: info})
  SymbolTable(
    types: new_types,
    cells: table.cells,
    cell_policies: table.cell_policies,
    tools: table.tools,
    agents: table.agents,
    processes: table.processes,
    effects: table.effects,
    effect_binds: table.effect_binds,
    handlers: table.handlers,
    addons: table.addons,
    type_aliases: table.type_aliases,
    traits: table.traits,
    impls: table.impls,
    consts: table.consts
  )
end
```
