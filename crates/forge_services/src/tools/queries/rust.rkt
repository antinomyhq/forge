;; Capture struct declarations
(struct_item
  name: (type_identifier) @definition.class.name
) @definition.class.declaration

;; Capture enum declarations
(enum_item
  name: (type_identifier) @definition.class.name
) @definition.class.declaration

;; Capture implementation blocks
(impl_item
  type: (type_identifier) @definition.class.name
) @definition.class

;; Capture function declarations
(function_item
  name: (identifier) @function.name
) @definition.function

;; Capture trait declarations
(trait_item
  name: (type_identifier) @definition.class.name
) @definition.class