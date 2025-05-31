;; Capture line comments and doc comments
(line_comment) @line_comment

;; Capture attributes
(attribute_item) @attribute_item

;; Capture struct definitions with their associated comments and attributes
(struct_item) @struct.definition

;; Capture enum definitions
(enum_item) @enum.definition

;; Capture function definitions
(function_item) @function.definition

;; Capture constant definitions
(const_item) @const.definition

;; type aliases
(type_item
    name: (type_identifier) @name) @type_alias.definition
