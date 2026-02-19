#!/usr/bin/env tsx

/**
 * LLM-as-Judge for Semantic Search Quality Evaluation
 *
 * This script evaluates the quality of semantic search queries and results
 * using Google's Gemini 3 Pro via Vertex AI as a judge. It verifies:
 * 1. Query quality - Are the embedding and reranking queries well-formed?
 * 2. Result relevance - Do the results match the stated intent?
 * 3. File type accuracy - Are the correct file types returned?
 * 4. Avoidance compliance - Are unwanted file types properly avoided?
 */

import { readFileSync } from "fs";
import { vertex } from "@ai-sdk/google-vertex";
import { generateObject } from "ai";
import { z } from "zod";

// Evaluation result schema
const EvaluationSchema = z.object({
  query_quality: z.object({
    embedding_query_score: z
      .number()
      .min(0)
      .max(15)
      .describe("Score for the embedding query quality (0-15)"),
    embedding_query_feedback: z
      .string()
      .describe("Specific feedback on the embedding query"),
    reranking_query_score: z
      .number()
      .min(0)
      .max(15)
      .describe("Score for the reranking query quality (0-15)"),
    reranking_query_feedback: z
      .string()
      .describe("Specific feedback on the reranking query"),
    construct_keywords_score: z
      .number()
      .min(0)
      .max(10)
      .describe(
        "Score for codebase construct keywords in reranking query (0-10)",
      ),
    construct_keywords_feedback: z
      .string()
      .describe(
        "Feedback on use of codebase constructs (struct, trait, impl, interface, class, function, definition, implementation, declaration)",
      ),
    queries_differentiated: z
      .boolean()
      .describe(
        "Whether the embedding and reranking queries are sufficiently different",
      ),
  }),
  result_relevance: z.object({
    intent_match_score: z
      .number()
      .min(0)
      .max(25)
      .describe("How well results match the stated intent (0-25)"),
    intent_match_feedback: z
      .string()
      .describe("Explanation of intent matching"),
    file_type_accuracy_score: z
      .number()
      .min(0)
      .max(20)
      .describe("Correctness of file types in results (0-20)"),
    file_type_feedback: z.string().describe("Feedback on file type selection"),
    avoidance_compliance_score: z
      .number()
      .min(0)
      .max(15)
      .describe("How well unwanted file types were avoided (0-15)"),
    avoidance_feedback: z.string().describe("Feedback on file type avoidance"),
  }),
  overall: z.object({
    passed: z
      .boolean()
      .describe("Whether the search quality passes the evaluation"),
    total_score: z
      .number()
      .min(0)
      .max(100)
      .describe("Total score (0-100)"),
    summary: z
      .string()
      .describe("Brief summary of the evaluation (2-3 sentences)"),
    critical_issues: z
      .array(z.string())
      .describe("List of critical issues that caused failure"),
  }),
});

type Evaluation = z.infer<typeof EvaluationSchema>;

interface ToolCall {
  id: string;
  type: string;
  function: {
    name: string;
    arguments: string;
  };
}

interface Message {
  tool_calls?: ToolCall[];
}

interface ContextData {
  messages: Message[];
}

interface Args {
  context: string;
  intent: string;
  expectedFileTypes: string;
  shouldAvoid: string;
}

function parseArgs(): Args {
  const args = process.argv.slice(2);
  const parsed: Record<string, string> = {};

  for (let i = 0; i < args.length; i += 2) {
    const key = args[i]?.replace(/^--/, "").replace(/-/g, "_");
    const value = args[i + 1];
    if (key && value) {
      parsed[key] = value;
    }
  }

  if (
    !parsed.context ||
    !parsed.intent ||
    !parsed.expected_file_types ||
    !parsed.should_avoid
  ) {
    console.error("Missing required arguments");
    console.error(
      "Usage: llm_judge.ts --context <file> --intent <intent> --expected-file-types <types> --should-avoid <types>",
    );
    process.exit(1);
  }

  return {
    context: parsed.context,
    intent: parsed.intent,
    expectedFileTypes: parsed.expected_file_types,
    shouldAvoid: parsed.should_avoid,
  };
}

function extractSemanticSearchCalls(contextData: ContextData): Array<{
  query: string;
  use_case: string;
}> {
  const searches: Array<{ query: string; use_case: string }> = [];

  for (const message of contextData.messages) {
    if (message.tool_calls) {
      for (const toolCall of message.tool_calls) {
        if (toolCall.function.name === "sem_search") {
          try {
            const args = JSON.parse(toolCall.function.arguments);
            if (args.queries && Array.isArray(args.queries)) {
              for (const q of args.queries) {
                if (q.query && q.use_case) {
                  searches.push({
                    query: q.query,
                    use_case: q.use_case,
                  });
                }
              }
            }
          } catch (e) {
            // Skip invalid JSON
          }
        }
      }
    }
  }

  return searches;
}

async function evaluateWithLLM(
  searches: Array<{ query: string; use_case: string }>,
  intent: string,
  expectedFileTypes: string,
  shouldAvoid: string,
): Promise<Evaluation> {
  const prompt = `You are an expert evaluator of semantic code search quality. Your task is to evaluate the quality of semantic search queries and assess whether they would return relevant results based on the user's intent.

## User's Intent
The user wants to: ${intent}

## Expected File Types
The search should primarily return files of these types: ${expectedFileTypes}

## File Types to Avoid
The search should avoid returning files of these types: ${shouldAvoid}

## Semantic Search Queries Generated
${searches.map((s, i) => `### Query ${i + 1}
**Embedding Query (for vector search):** ${s.query}
**Reranking Query (for result filtering):** ${s.use_case}
`).join("\n")}

## Evaluation Criteria

### Query Quality (50 points)
1. **Embedding Query (15 points):**
   - Contains domain-specific terms and technical context
   - Describes behavior and functionality, not meta-descriptions
   - Is complete and descriptive, not overly generic
   - Would generate embeddings that match similar code

2. **Reranking Query (15 points):**
   - Expresses the specific intent and use case
   - Provides context about WHY the code is needed
   - Specifies whether implementation/docs/tests are needed
   - Is different from the embedding query (not verbatim)

3. **Codebase Construct Keywords (10 points):**
   - **CRITICAL**: When looking for code (not documentation), the reranking query MUST include specific construct keywords
   - These keywords get HIGH WEIGHTAGE in reranking: struct, trait, impl, interface, class, function, definition, implementation, declaration, type
   - Examples of GOOD use_case queries:
     * "I need the struct definition for User authentication"
     * "Show me the trait implementation for DatabaseConnection"
     * "Find the function implementation that handles file patching"
     * "I need the type declarations and interface definitions for the tool registry"
   - Examples of BAD use_case queries (missing construct keywords):
     * "I need code that handles authentication" (missing: struct/trait/impl/function)
     * "Show me the database logic" (missing: trait/impl/function)
     * "Find file patching code" (missing: function/impl/struct)
   - **Scoring**:
     * 10 points: Multiple specific construct keywords (struct, trait, impl, function, etc.)
     * 7-9 points: At least one specific construct keyword
     * 4-6 points: Generic keywords (code, implementation) without specifics
     * 0-3 points: No construct keywords at all
   - **Exception**: Documentation/config intents don't need construct keywords

4. **Query Differentiation (10 points):**
   - The two queries serve different purposes
   - Embedding query focuses on WHAT (semantic similarity)
   - Reranking query focuses on INTENT + CONSTRUCTS (contextual relevance + code structure)

### Result Relevance (50 points)
Based on the queries, evaluate whether they would return appropriate results:

1. **Intent Matching (25 points):**
   - Would the queries find code matching the user's intent?
   - For "implementation" intent: would it find actual code, not docs?
   - For "documentation" intent: would it find docs, not code?
   - For "tests" intent: would it find test files?
   - For "flow_understanding": would it find the relevant code flow?
   - For "debugging": would it find the problematic areas?
   - For "modification": would it find the right code to modify?

2. **File Type Accuracy (20 points):**
   - Would the queries naturally return the expected file types?
   - Are the queries specific enough to filter for the right extensions?

3. **Avoidance Compliance (15 points):**
   - Would the queries avoid returning the unwanted file types?
   - Is the intent clear enough to prevent irrelevant results?

## Scoring Guidelines
- **9-10:** Excellent - Professional quality, no significant issues
- **7-8:** Good - Minor improvements possible but functional
- **5-6:** Adequate - Some issues but would work in most cases
- **3-4:** Poor - Significant issues that impact effectiveness
- **0-2:** Failing - Critical problems, would not work as intended

## Pass/Fail Criteria
- **PASS:** Total score >= 70/100 AND no critical issues
- **FAIL:** Total score < 70/100 OR has critical issues

Critical issues include:
- Embedding and reranking queries are identical or nearly identical
- Query is too generic to return relevant results
- Intent mismatch is severe (e.g., asking for implementation but would return docs)
- Would definitely return wrong file types for the intent

Please evaluate these queries and provide detailed, constructive feedback.`;

  try {
    // @ts-ignore - Type instantiation depth issue with complex Zod schemas
    const result = await generateObject({
      model: vertex("gemini-3-pro-preview"),
      schema: EvaluationSchema,
      prompt,
      temperature: 0.3, // Lower temperature for more consistent evaluations
    });

    return result.object as Evaluation;
  } catch (error) {
    console.error("Error calling Gemini API:", error);
    throw error;
  }
}

function formatEvaluation(evaluation: Evaluation): string {
  const lines: string[] = [];

  lines.push("=== SEMANTIC SEARCH QUALITY EVALUATION ===\n");

  // Query Quality
  lines.push("## Query Quality");
  lines.push(
    `Embedding Query: ${evaluation.query_quality.embedding_query_score}/15`,
  );
  lines.push(`  ${evaluation.query_quality.embedding_query_feedback}`);
  lines.push(
    `Reranking Query: ${evaluation.query_quality.reranking_query_score}/15`,
  );
  lines.push(`  ${evaluation.query_quality.reranking_query_feedback}`);
  lines.push(
    `Construct Keywords: ${evaluation.query_quality.construct_keywords_score}/10`,
  );
  lines.push(`  ${evaluation.query_quality.construct_keywords_feedback}`);
  lines.push(
    `Queries Differentiated: ${evaluation.query_quality.queries_differentiated ? "✓ Yes" : "✗ No"}`,
  );
  lines.push("");

  // Result Relevance
  lines.push("## Result Relevance");
  lines.push(
    `Intent Match: ${evaluation.result_relevance.intent_match_score}/25`,
  );
  lines.push(`  ${evaluation.result_relevance.intent_match_feedback}`);
  lines.push(
    `File Type Accuracy: ${evaluation.result_relevance.file_type_accuracy_score}/20`,
  );
  lines.push(`  ${evaluation.result_relevance.file_type_feedback}`);
  lines.push(
    `Avoidance Compliance: ${evaluation.result_relevance.avoidance_compliance_score}/15`,
  );
  lines.push(`  ${evaluation.result_relevance.avoidance_feedback}`);
  lines.push("");

  // Overall
  lines.push("## Overall Assessment");
  lines.push(`Total Score: ${evaluation.overall.total_score}/100`);
  lines.push(`Status: ${evaluation.overall.passed ? "✓ PASS" : "✗ FAIL"}`);
  lines.push(`Summary: ${evaluation.overall.summary}`);

  if (evaluation.overall.critical_issues.length > 0) {
    lines.push("\n## Critical Issues:");
    for (const issue of evaluation.overall.critical_issues) {
      lines.push(`  - ${issue}`);
    }
  }

  return lines.join("\n");
}

async function main() {
  const args = parseArgs();

  // Read context file
  let contextData: ContextData;
  try {
    const contextContent = readFileSync(args.context, "utf-8");
    contextData = JSON.parse(contextContent);
  } catch (error) {
    console.error(`Error reading context file: ${error}`);
    process.exit(1);
  }

  // Extract semantic search calls
  const searches = extractSemanticSearchCalls(contextData);

  if (searches.length === 0) {
    console.error("No semantic search calls found in context");
    process.exit(1);
  }

  // Evaluate with LLM
  const evaluation = await evaluateWithLLM(
    searches,
    args.intent,
    args.expectedFileTypes,
    args.shouldAvoid,
  );

  // Output results
  console.log(formatEvaluation(evaluation));

  // Exit with appropriate code
  process.exit(evaluation.overall.passed ? 0 : 1);
}

main().catch((error) => {
  console.error("Unexpected error:", error);
  process.exit(1);
});
