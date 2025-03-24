;; Capture class declarations
(class_definition
    name: (identifier) @name.definition.class
) @definition.class

;; Capture object declarations
(object_definition
    name: (identifier) @name.definition.object
) @definition.object

;; Capture trait declarations
(trait_definition
    name: (identifier) @name.definition.trait
) @definition.trait

;; Capture case class declarations
(class_definition
    "case"
    name: (identifier) @name.definition.case_class
) @definition.case_class

;; Capture functions and methods
(function_definition
    name: (identifier) @name.definition.function
) @definition.function

;; Capture val declarations
(val_definition
    pattern: (identifier) @name.definition.val
) @definition.val

;; Capture type declarations
(type_definition
    name: (identifier) @name.definition.type
) @definition.type
