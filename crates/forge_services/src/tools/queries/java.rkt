;; Capture class declarations
(class_declaration
    name: (identifier) @name.definition.class
) @definition.class

;; Capture interface declarations
(interface_declaration
    name: (identifier) @name.definition.interface
) @definition.interface

;; Capture methods
(method_declaration
    name: (identifier) @name.definition.method
) @definition.method

;; Capture constructors
(constructor_declaration
    name: (identifier) @name.definition.constructor
) @definition.constructor

;; Capture enum declarations
(enum_declaration
    name: (identifier) @name.definition.enum
) @definition.enum

;; Capture record declarations (Java 14+)
(record_declaration
    name: (identifier) @name.definition.record
) @definition.record

;; Capture annotations
(annotation_type_declaration
    name: (identifier) @name.definition.annotation
) @definition.annotation