type BaseNode = {
  type: string;
  named: boolean;
};

type ChildNode = {
  multiple: boolean;
  required: boolean;
  types: BaseNode[];
};

type NodeInfo =
  | (BaseNode & {
      subtypes: BaseNode[];
    })
  | (BaseNode & {
      fields: { [name: string]: ChildNode };
      children: ChildNode;
    });

/**
 * The tree-sitter language object for this grammar.
 */
declare const language: {
  name: string;
  language: unknown;
  nodeTypeInfo: NodeInfo[];
};

export = language;
