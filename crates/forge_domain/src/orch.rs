use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Context as _;
use async_recursion::async_recursion;
use futures::future::join_all;
use futures::Stream;
use serde_json::Value;

use crate::arena::{Arena, SmartTool};
use crate::{
    Agent, AgentId, ChatCompletionMessage, ContentMessage, Context, ContextMessage, Error, FlowId,
    ProviderService, Role, Summarize, SystemContext, ToolCallFull, ToolDefinition, ToolName,
    ToolResult, ToolService, Transform, Variables, Workflow, WorkflowId,
};

pub struct Orchestrator {
    arena: Arena,
    system_context: SystemContext,
    provider: Arc<dyn ProviderService>,
    tool: Arc<dyn ToolService>,
}

impl Orchestrator {
    pub fn new(
        provider: Arc<dyn ProviderService>,
        tool: Arc<dyn ToolService>,
        arena: Arena,
        system_context: SystemContext,
    ) -> Self {
        Self { arena, provider, system_context, tool }
    }

    pub async fn execute(&self, id: &FlowId, input: &Variables) -> anyhow::Result<Variables> {
        match id {
            FlowId::Agent(id) => self.init_agent(id, input).await,
            FlowId::Workflow(id) => self.init_workflow(id, input).await,
        }
    }

    fn init_default_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool.list()
    }

    fn init_smart_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.arena
            .tools
            .iter()
            .map(|a| a.to_tool_definition())
            .collect()
    }

    fn init_tool_definitions(&self, tools: &[ToolName]) -> Vec<ToolDefinition> {
        let required_tools = tools.iter().collect::<HashSet<_>>();
        let default_tools = self.init_default_tool_definitions();
        let smart_tools = self.init_smart_tool_definitions();

        default_tools
            .into_iter()
            .chain(smart_tools)
            .filter(|tool| required_tools.contains(&tool.name))
            .collect::<Vec<_>>()
    }

    fn init_agent_context(&self, agent: &Agent, input: &Variables) -> anyhow::Result<Context> {
        let tool_defs = self.init_tool_definitions(&agent.tools);

        let tool_usage_prompt = tool_defs.iter().fold("".to_string(), |acc, tool| {
            format!("{}\n{}", acc, tool.usage_prompt())
        });

        let system_message = agent.system_prompt.render(
            &self
                .system_context
                .clone()
                .tool_information(tool_usage_prompt),
        )?;

        let user_message = ContextMessage::user(agent.user_prompt.render(input)?);

        Ok(Context::default()
            .set_first_system_message(system_message)
            .add_message(user_message)
            .extend_tools(tool_defs))
    }

    async fn collect_content(
        &self,
        _response: &impl Stream<Item = std::result::Result<ChatCompletionMessage, anyhow::Error>>,
    ) -> String {
        todo!()
    }

    async fn collect_tool_calls(
        &self,
        _response: &impl Stream<Item = std::result::Result<ChatCompletionMessage, anyhow::Error>>,
        _variables: &mut Variables,
    ) -> Vec<ToolCallFull> {
        todo!()
    }

    fn find_tool(&self, name: &ToolName) -> Option<&SmartTool<Variables>> {
        self.arena.tools.iter().find(|tool| tool.name == *name)
    }

    async fn execute_tool(&self, tool_call: &ToolCallFull) -> anyhow::Result<ToolResult> {
        let smart_tool = self.find_tool(&tool_call.name);
        if let Some(tool) = smart_tool {
            let input = Variables::from(tool_call.arguments.clone());
            match self.execute(&tool.run, &input).await {
                Ok(output) => {
                    let output = serde_json::to_string(&output).with_context(|| {
                        format!(
                            "Failed to serialize output of smart tool: {}",
                            tool.name.as_str()
                        )
                    })?;
                    Ok(ToolResult::from(tool_call.clone()).success(output))
                }
                Err(error) => Ok(ToolResult::from(tool_call.clone()).failure(error.to_string())),
            }
        } else {
            Ok(self.tool.call(tool_call.clone()).await)
        }
    }

    fn find_workflow(&self, id: &WorkflowId) -> anyhow::Result<&Workflow> {
        Ok(self
            .arena
            .workflows
            .iter()
            .find(|w| w.id == *id)
            .ok_or(Error::WorkflowUndefined(id.clone()))?)
    }

    fn find_agent(&self, id: &AgentId) -> anyhow::Result<&Agent> {
        Ok(self
            .arena
            .agents
            .iter()
            .find(|a| a.id == *id)
            .ok_or(Error::AgentUndefined(id.clone()))?)
    }

    #[async_recursion(?Send)]
    async fn execute_transform(
        &self,
        transforms: &[Transform],
        mut context: Context,
    ) -> anyhow::Result<Context> {
        for transform in transforms.iter() {
            match transform {
                Transform::Assistant {
                    agent_id,
                    token_limit,
                    input: input_key,
                    output: output_key,
                } => {
                    let mut summarize = Summarize::new(&mut context, *token_limit);
                    while let Some(mut summary) = summarize.summarize() {
                        let mut input = Variables::default();
                        input.add(input_key, summary.get());

                        let output = self.init_agent(agent_id, &input).await?;
                        let value = output
                            .get(output_key)
                            .ok_or(Error::UndefinedVariable(output_key.to_string()))?;

                        summary.set(serde_json::to_string(&value)?);
                    }
                }
                Transform::User { agent_id, input: input_key, output: output_key } => {
                    if let Some(ContextMessage::ContentMessage(ContentMessage {
                        role: Role::User,
                        content,
                        ..
                    })) = context.messages.last_mut()
                    {
                        let mut input = Variables::default();
                        input.add(input_key, Value::from(content.clone()));

                        let output = self.init_agent(agent_id, &input).await?;
                        let value = output
                            .get(output_key)
                            .ok_or(Error::UndefinedVariable(output_key.to_string()))?;

                        let message = serde_json::to_string(&value)?;

                        content.push_str(&format!("\n<{output_key}>\n{message}\n</{output_key}>"));
                    }
                }
                Transform::Tap { agent_id, input: input_key } => {
                    let mut input = Variables::default();
                    input.add(input_key, context.to_text());

                    // NOTE: Tap transformers will not modify the context
                    self.init_agent(agent_id, &input).await?;
                }
            }
        }

        Ok(context)
    }

    async fn init_agent(&self, agent: &AgentId, input: &Variables) -> anyhow::Result<Variables> {
        let agent = self.find_agent(agent)?;
        let mut context = self.init_agent_context(agent, input)?;
        let content = agent.user_prompt.render(input)?;
        let mut output = Variables::default();
        context = context.add_message(ContextMessage::user(content));

        loop {
            context = self.execute_transform(&agent.transforms, context).await?;

            let response = self.provider.chat(&agent.model, context.clone()).await?;
            let tool_calls = self.collect_tool_calls(&response, &mut output).await;

            if tool_calls.is_empty() {
                return Ok(output);
            }

            let content = self.collect_content(&response).await;

            let tool_results = join_all(
                tool_calls
                    .iter()
                    .map(|tool_call| self.execute_tool(tool_call)),
            )
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()?;

            context = context
                .add_message(ContextMessage::assistant(content, Some(tool_calls)))
                .add_tool_results(tool_results);
        }
    }

    async fn init_flow(
        &self,
        flow_id: &FlowId,
        input: &Variables,
        workflow: &Workflow,
    ) -> anyhow::Result<Variables> {
        match flow_id {
            FlowId::Agent(agent_id) => {
                let variables = self.init_agent(agent_id, input).await?;
                let flows = workflow
                    .handovers
                    .get(&agent_id.clone().into())
                    .ok_or(Error::AgentUndefined(agent_id.clone()))?;

                join_all(
                    flows
                        .iter()
                        .map(|flow_id| self.init_flow(flow_id, &variables, workflow)),
                )
                .await
                .into_iter()
                .collect::<anyhow::Result<Vec<_>>>()
                .map(Variables::from)
            }
            FlowId::Workflow(id) => self.init_workflow(id, input).await,
        }
    }

    async fn init_workflow(&self, id: &WorkflowId, input: &Variables) -> anyhow::Result<Variables> {
        let workflow = self.find_workflow(id)?;
        join_all(
            workflow
                .head_flow()
                .iter()
                .map(|flow_id| self.init_flow(flow_id, input, workflow)),
        )
        .await
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()
        .map(Variables::from)
    }
}
