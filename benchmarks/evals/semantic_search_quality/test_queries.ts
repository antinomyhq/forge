#!/usr/bin/env tsx

/**
 * Manual Query Tester for Semantic Search
 * 
 * This script helps developers test and improve their semantic search queries
 * before committing them to the codebase using Gemini 3 Pro via Vertex AI.
 */

import { vertex } from "@ai-sdk/google-vertex";
import { generateObject } from "ai";
import { z } from "zod";
import * as readline from "readline";

const QueryEvaluationSchema = z.object({
  embedding_query: z.object({
    score: z.number().min(0).max(10),
    strengths: z.array(z.string()),
    weaknesses: z.array(z.string()),
    suggestions: z.array(z.string()),
  }),
  reranking_query: z.object({
    score: z.number().min(0).max(10),
    strengths: z.array(z.string()),
    weaknesses: z.array(z.string()),
    suggestions: z.array(z.string()),
  }),
  differentiation: z.object({
    are_different: z.boolean(),
    explanation: z.string(),
    suggestion: z.string().optional(),
  }),
  overall_recommendation: z.string(),
});

async function evaluateQueries(
  embeddingQuery: string,
  rerankingQuery: string,
): Promise<z.infer<typeof QueryEvaluationSchema>> {
  const prompt = `You are an expert in semantic code search. Evaluate these two queries:

**Embedding Query (for vector search):**
${embeddingQuery}

**Reranking Query (for result filtering):**
${rerankingQuery}

Provide detailed, constructive feedback on:
1. The quality of each query
2. Whether they are sufficiently different
3. How to improve them

Scoring (0-10):
- 9-10: Excellent
- 7-8: Good
- 5-6: Adequate
- 3-4: Poor
- 0-2: Failing

Focus on practical, actionable suggestions.`;

  // @ts-ignore - Type instantiation depth issue with complex Zod schemas
  const result = await generateObject({
    model: vertex("gemini-3-pro-preview"),
    schema: QueryEvaluationSchema,
    prompt,
    temperature: 0.3,
  });
  return result.object as z.infer<typeof QueryEvaluationSchema>;
}

function formatEvaluation(evaluation: z.infer<typeof QueryEvaluationSchema>): string {
  const lines: string[] = [];

  lines.push("\n╔═══════════════════════════════════════════════════════════╗");
  lines.push("║          SEMANTIC SEARCH QUERY EVALUATION                 ║");
  lines.push("╚═══════════════════════════════════════════════════════════╝\n");

  // Embedding Query
  lines.push("📊 EMBEDDING QUERY ANALYSIS");
  lines.push(`   Score: ${evaluation.embedding_query.score}/10 ${getScoreEmoji(evaluation.embedding_query.score)}\n`);
  
  if (evaluation.embedding_query.strengths.length > 0) {
    lines.push("   ✅ Strengths:");
    for (const strength of evaluation.embedding_query.strengths) {
      lines.push(`      • ${strength}`);
    }
    lines.push("");
  }

  if (evaluation.embedding_query.weaknesses.length > 0) {
    lines.push("   ⚠️  Weaknesses:");
    for (const weakness of evaluation.embedding_query.weaknesses) {
      lines.push(`      • ${weakness}`);
    }
    lines.push("");
  }

  if (evaluation.embedding_query.suggestions.length > 0) {
    lines.push("   💡 Suggestions:");
    for (const suggestion of evaluation.embedding_query.suggestions) {
      lines.push(`      • ${suggestion}`);
    }
    lines.push("");
  }

  // Reranking Query
  lines.push("📊 RERANKING QUERY ANALYSIS");
  lines.push(`   Score: ${evaluation.reranking_query.score}/10 ${getScoreEmoji(evaluation.reranking_query.score)}\n`);

  if (evaluation.reranking_query.strengths.length > 0) {
    lines.push("   ✅ Strengths:");
    for (const strength of evaluation.reranking_query.strengths) {
      lines.push(`      • ${strength}`);
    }
    lines.push("");
  }

  if (evaluation.reranking_query.weaknesses.length > 0) {
    lines.push("   ⚠️  Weaknesses:");
    for (const weakness of evaluation.reranking_query.weaknesses) {
      lines.push(`      • ${weakness}`);
    }
    lines.push("");
  }

  if (evaluation.reranking_query.suggestions.length > 0) {
    lines.push("   💡 Suggestions:");
    for (const suggestion of evaluation.reranking_query.suggestions) {
      lines.push(`      • ${suggestion}`);
    }
    lines.push("");
  }

  // Differentiation
  lines.push("🔄 QUERY DIFFERENTIATION");
  lines.push(`   ${evaluation.differentiation.are_different ? "✅" : "❌"} Queries are ${evaluation.differentiation.are_different ? "sufficiently different" : "too similar"}`);
  lines.push(`   ${evaluation.differentiation.explanation}\n`);
  
  if (evaluation.differentiation.suggestion) {
    lines.push(`   💡 ${evaluation.differentiation.suggestion}\n`);
  }

  // Overall
  lines.push("🎯 OVERALL RECOMMENDATION");
  lines.push(`   ${evaluation.overall_recommendation}\n`);

  return lines.join("\n");
}

function getScoreEmoji(score: number): string {
  if (score >= 9) return "🌟";
  if (score >= 7) return "👍";
  if (score >= 5) return "👌";
  if (score >= 3) return "⚠️";
  return "❌";
}

async function promptUser(question: string): Promise<string> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  return new Promise((resolve) => {
    rl.question(question, (answer) => {
      rl.close();
      resolve(answer.trim());
    });
  });
}

async function main() {
  console.log("\n╔═══════════════════════════════════════════════════════════╗");
  console.log("║     SEMANTIC SEARCH QUERY TESTER                          ║");
  console.log("║     Test and improve your queries with AI feedback        ║");
  console.log("╚═══════════════════════════════════════════════════════════╝\n");

  // Get queries from user
  const embeddingQuery = await promptUser(
    "Enter your embedding query (for vector search):\n> ",
  );
  
  if (!embeddingQuery) {
    console.error("Embedding query is required");
    process.exit(1);
  }

  console.log("");
  const rerankingQuery = await promptUser(
    "Enter your reranking query (for result filtering):\n> ",
  );

  if (!rerankingQuery) {
    console.error("Reranking query is required");
    process.exit(1);
  }

  console.log("\n🔄 Evaluating your queries...\n");

  try {
    const evaluation = await evaluateQueries(embeddingQuery, rerankingQuery);
    console.log(formatEvaluation(evaluation));
  } catch (error) {
    console.error("Error evaluating queries:", error);
    process.exit(1);
  }
}

// Check if running in interactive mode or with args
const args = process.argv.slice(2);
if (args.length >= 2) {
  // Non-interactive mode
  const embeddingQuery = args[0];
  const rerankingQuery = args[1];
  
  console.log("\n🔄 Evaluating your queries...\n");
  evaluateQueries(embeddingQuery!, rerankingQuery!)
    .then((evaluation) => {
      console.log(formatEvaluation(evaluation));
    })
    .catch((error) => {
      console.error("Error evaluating queries:", error);
      process.exit(1);
    });
} else {
  // Interactive mode
  main().catch((error) => {
    console.error("Unexpected error:", error);
    process.exit(1);
  });
}
