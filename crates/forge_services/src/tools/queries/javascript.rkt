;; Capture class declarations with their methods
(
    (comment)* @doc
    .
    [
        (class
            name: (_) @name)
        (class_declaration
            name: (_) @name)
    ] @definition.class
    (#strip! @doc "^[\\s\\*/]+|^[\\s\\*/]$")
    (#select-adjacent! @doc @definition.class)
)

;; Capture methods (excluding constructor)
(
    (comment)* @doc
    .
    (method_definition
        name: (property_identifier) @name) @definition.method
    (#not-eq? @name "constructor")
    (#strip! @doc "^[\\s\\*/]+|^[\\s\\*/]$")
    (#select-adjacent! @doc @definition.method)
)

;; Capture all function declarations including generators and async
(
    (comment)* @doc
    .
    [
        (function_declaration
            name: (identifier) @name)
        (generator_function_declaration
            name: (identifier) @name)
    ] @definition.function
    (#strip! @doc "^[\\s\\*/]+|^[\\s\\*/]$")
    (#select-adjacent! @doc @definition.function)
)

;; Capture arrow functions and function expressions assigned to variables
(
    (comment)* @doc
    .
    [
        (lexical_declaration
            (variable_declarator
                name: (identifier) @name
                value: [(arrow_function) (function_expression)]))
        (variable_declaration
            (variable_declarator
                name: (identifier) @name
                value: [(arrow_function) (function_expression)]))
    ] @definition.function
    (#strip! @doc "^[\\s\\*/]+|^[\\s\\*/]$")
    (#select-adjacent! @doc @definition.function)
)