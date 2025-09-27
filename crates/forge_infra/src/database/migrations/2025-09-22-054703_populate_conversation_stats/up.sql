INSERT INTO conversation_stats (
    conversation_id,
    workspace_id,
    title,
    message_count,
    total_tokens,
    prompt_tokens,
    completion_tokens,
    cached_tokens,
    cost,
    created_at,
    updated_at
)
SELECT 
    c.conversation_id,
    c.workspace_id,
    c.title,
    -- Safely extract message count with fallback to 0
    COALESCE(
        CASE 
            WHEN c.context IS NOT NULL 
                 AND c.context != '' 
                 AND json_valid(c.context) = 1
                 AND json_extract(c.context, '$.messages') IS NOT NULL
            THEN json_array_length(json_extract(c.context, '$.messages'))
            ELSE 0
        END,
        0
    ) AS message_count,
    -- Safely extract total_tokens with enum handling and fallback
    COALESCE(
        CASE 
            WHEN c.context IS NOT NULL 
                 AND c.context != '' 
                 AND json_valid(c.context) = 1
                 AND json_extract(c.context, '$.usage.total_tokens') IS NOT NULL
            THEN CASE
                WHEN json_type(json_extract(c.context, '$.usage.total_tokens')) = 'object'
                THEN CAST(COALESCE(
                    json_extract(c.context, '$.usage.total_tokens.Actual'),
                    json_extract(c.context, '$.usage.total_tokens.Approx'),
                    0
                ) AS INTEGER)
                ELSE CAST(json_extract(c.context, '$.usage.total_tokens') AS INTEGER)
            END
            ELSE 0
        END,
        0
    ) AS total_tokens,
    -- Safely extract prompt_tokens with enum handling and fallback
    COALESCE(
        CASE 
            WHEN c.context IS NOT NULL 
                 AND c.context != '' 
                 AND json_valid(c.context) = 1
                 AND json_extract(c.context, '$.usage.prompt_tokens') IS NOT NULL
            THEN CASE
                WHEN json_type(json_extract(c.context, '$.usage.prompt_tokens')) = 'object'
                THEN CAST(COALESCE(
                    json_extract(c.context, '$.usage.prompt_tokens.Actual'),
                    json_extract(c.context, '$.usage.prompt_tokens.Approx'),
                    0
                ) AS INTEGER)
                ELSE CAST(json_extract(c.context, '$.usage.prompt_tokens') AS INTEGER)
            END
            ELSE 0
        END,
        0
    ) AS prompt_tokens,
    -- Safely extract completion_tokens with enum handling and fallback
    COALESCE(
        CASE 
            WHEN c.context IS NOT NULL 
                 AND c.context != '' 
                 AND json_valid(c.context) = 1
                 AND json_extract(c.context, '$.usage.completion_tokens') IS NOT NULL
            THEN CASE
                WHEN json_type(json_extract(c.context, '$.usage.completion_tokens')) = 'object'
                THEN CAST(COALESCE(
                    json_extract(c.context, '$.usage.completion_tokens.Actual'),
                    json_extract(c.context, '$.usage.completion_tokens.Approx'),
                    0
                ) AS INTEGER)
                ELSE CAST(json_extract(c.context, '$.usage.completion_tokens') AS INTEGER)
            END
            ELSE 0
        END,
        0
    ) AS completion_tokens,
    -- Safely extract cached_tokens with enum handling and fallback
    COALESCE(
        CASE 
            WHEN c.context IS NOT NULL 
                 AND c.context != '' 
                 AND json_valid(c.context) = 1
                 AND json_extract(c.context, '$.usage.cached_tokens') IS NOT NULL
            THEN CASE
                WHEN json_type(json_extract(c.context, '$.usage.cached_tokens')) = 'object'
                THEN CAST(COALESCE(
                    json_extract(c.context, '$.usage.cached_tokens.Actual'),
                    json_extract(c.context, '$.usage.cached_tokens.Approx'),
                    0
                ) AS INTEGER)
                ELSE CAST(json_extract(c.context, '$.usage.cached_tokens') AS INTEGER)
            END
            ELSE 0
        END,
        0
    ) AS cached_tokens,
    -- Safely extract cost with fallback to 0.0
    COALESCE(
        CASE 
            WHEN c.context IS NOT NULL 
                 AND c.context != '' 
                 AND json_valid(c.context) = 1
                 AND json_extract(c.context, '$.usage.cost') IS NOT NULL
            THEN CAST(json_extract(c.context, '$.usage.cost') AS REAL)
            ELSE 0.0
        END,
        0.0
    ) as cost,
    c.created_at,
    c.updated_at
FROM conversations c
WHERE NOT EXISTS (
    -- Avoid duplicate inserts if migration is run multiple times
    SELECT 1 FROM conversation_stats cs WHERE cs.conversation_id = c.conversation_id
);