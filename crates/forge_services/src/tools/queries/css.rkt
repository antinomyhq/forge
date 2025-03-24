;; Capture selectors and their nested selectors
(rule_set
    (selectors
        (class_selector) @name.definition.class
    )
) @definition.rule

;; Capture @media queries
(media_statement
    (feature_query
        (feature_name) @name.definition.media)
) @definition.media

;; Capture @keyframes
(keyframes_statement
    (_) @name.definition.keyframes
) @definition.keyframes

;; Capture variables and custom properties
(declaration
    (property_name) @name.definition.property
    (#match? @name.definition.property "^--")
) @definition.property
