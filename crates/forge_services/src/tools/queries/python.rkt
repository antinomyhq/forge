;; Capture class definitions
(class_definition
    name: (identifier) @name.definition.class
) @definition.class

;; Capture all function definitions
(function_definition
    name: (identifier) @name.definition.function
) @definition.function

;; Capture class methods
(class_definition
    body: (block
        (function_definition
            name: (identifier) @name.definition.method)
    )
)