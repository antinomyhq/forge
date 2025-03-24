;; Capture interfaces
(interface_declaration
    name: (type_identifier) @name.definition.interface
) @definition.interface

;; Capture types
(type_alias_declaration
    name: (type_identifier) @name.definition.type
) @definition.type

;; Capture class declarations with methods
(
    [
        (class_declaration
            name: [(type_identifier) (identifier)] @name)
        (abstract_class_declaration
            name: [(type_identifier) (identifier)] @name)
    ] @definition.class
)

;; Capture React Components (Function and Class based)
(
    [
        (function_declaration
            name: (identifier) @name)
        (variable_declarator
            name: (identifier) @name
            value: [(arrow_function) (function_expression)])
    ] @definition.component
    (#match? @name "^[A-Z]")  ;; React components start with capital letter
)

;; Capture methods (excluding constructor)
(
    (method_definition
        name: (property_identifier) @name) @definition.method
    (#not-eq? @name "constructor")
)

;; Capture all functions including arrow functions and async
(
    [
        (function_declaration
            name: (identifier) @name)
        (generator_function_declaration
            name: (identifier) @name)
        (variable_declarator
            name: (identifier) @name
            value: [(arrow_function) (function_expression)])
    ] @definition.function
)

;; Capture enums
(enum_declaration
    name: (identifier) @name.definition.enum
) @definition.enum
