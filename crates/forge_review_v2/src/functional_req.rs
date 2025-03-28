use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("Missing title in document")]
    MissingTitle,
    #[error("Missing overview section")]
    MissingOverview,
    #[error("Invalid requirement format: {0}")]
    InvalidRequirement(String),
    #[error("Invalid dependency format: {0}")]
    InvalidDependency(String),
    #[error("Invalid section structure: {0}")]
    InvalidStructure(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionalRequirements {
    pub title: String,
    pub overview: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<Requirement>,
    #[serde(skip_serializing_if = "OperationOrdering::is_empty")]
    pub operation_ordering: OperationOrdering,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ambiguities: Vec<Ambiguity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Requirement {
    pub id: String,
    pub name: String,
    pub statement: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub state_changes: String,
    #[serde(skip_serializing_if = "RequirementOrdering::is_empty")]
    pub ordering: RequirementOrdering,
    #[serde(skip_serializing_if = "RequirementDependencies::is_empty")]
    pub dependencies: RequirementDependencies,
    #[serde(skip_serializing_if = "RequirementInvariants::is_empty")]
    pub invariants: RequirementInvariants,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub acceptance_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequirementOrdering {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub must_occur_before: Vec<OrderingConstraint>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub must_occur_after: Vec<OrderingConstraint>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub must_not_occur_before: Vec<OrderingConstraint>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub must_not_occur_after: Vec<OrderingConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderingConstraint {
    pub requirement_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequirementDependencies {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub state_dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub event_dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub temporal_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequirementInvariants {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub state_invariants: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub temporal_invariants: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OperationOrdering {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub state_transition_rules: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub side_effect_rules: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub temporal_invariants: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    pub from_req: String,
    pub to_req: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ambiguity {
    pub id: String,
    pub description: String,
}

impl std::fmt::Display for Requirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "id: {}", self.id)?;
        writeln!(f, "name: {}", self.name)?;
        writeln!(f, "category: {}", self.category)?;
        writeln!(f, "statement: {}", self.statement)?;
        writeln!(f, "state_changes: {}", self.state_changes)?;
        writeln!(
            f,
            "acceptance_criteria: {}",
            self.acceptance_criteria.join(", ")
        )?;
        Ok(())
    }
}

#[derive(Debug)]
enum ParsingState {
    Initial,
    Overview,
    Requirements,
    OperationOrdering { substate: OperationOrderingState },
    Dependencies,
    Ambiguities,
}

#[derive(Debug)]
enum OperationOrderingState {
    Initial,
    StateTransitionRules,
    SideEffectRules,
    TemporalInvariants,
}

#[derive(Debug)]
enum RequirementParsingState {
    Initial,
    Statement,
    Category,
    StateChanges,
    Ordering,
    Dependencies,
    Invariants,
    AcceptanceCriteria,
}

impl RequirementParsingState {
    fn from_strong_text(text: &str) -> Self {
        match text {
            "Requirement:" => RequirementParsingState::Statement,
            "Category:" => RequirementParsingState::Category,
            "State Changes:" => RequirementParsingState::StateChanges,
            "Ordering:" => RequirementParsingState::Ordering,
            "Dependencies:" => RequirementParsingState::Dependencies,
            "Invariants:" => RequirementParsingState::Invariants,
            "Acceptance Criteria:" => RequirementParsingState::AcceptanceCriteria,
            _ => RequirementParsingState::Initial,
        }
    }
}

impl FunctionalRequirements {
    pub fn parse(content: &str) -> Result<Self, ParserError> {
        let parser = Parser::new(content);
        let mut state = ParsingState::Initial;
        let mut req_state = RequirementParsingState::Initial;

        let mut title = String::new();
        let mut overview = String::new();
        let mut requirements = Vec::new();
        let mut operation_ordering = OperationOrdering::default();
        let mut dependencies = Vec::new();
        let mut ambiguities = Vec::new();

        let mut current_req = Requirement::default();
        let mut current_text = String::new();
        let mut list_level = 0;
        let mut current_dependency_type: Option<String> = None;

        for event in parser {
            match event {
                Event::Start(Tag::Heading(level, ..)) => match level {
                    HeadingLevel::H1 => {
                        state = ParsingState::Initial;
                        current_text.clear();
                    }
                    HeadingLevel::H2 => {
                        if !current_req.id.is_empty() {
                            requirements.push(current_req.clone());
                            current_req = Requirement::default();
                        }
                        current_text.clear();
                    }
                    HeadingLevel::H3 => {
                        if matches!(state, ParsingState::Requirements) {
                            if !current_req.id.is_empty() {
                                requirements.push(current_req.clone());
                            }
                            current_req = Requirement::default();
                            req_state = RequirementParsingState::Initial;
                        }
                        current_text.clear();
                    }
                    _ => {}
                },
                Event::End(Tag::Heading(level, ..)) => {
                    let text = current_text.trim();
                    match level {
                        HeadingLevel::H1 => {
                            if let Some(t) = text.strip_prefix("Functional Requirements:") {
                                title = t.trim().to_string();
                            }
                        }
                        HeadingLevel::H2 => match text {
                            "Overview" => state = ParsingState::Overview,
                            "Functional Requirements" => state = ParsingState::Requirements,
                            "Operation Ordering" => {
                                state = ParsingState::OperationOrdering {
                                    substate: OperationOrderingState::Initial,
                                };
                            }
                            "Dependencies" => state = ParsingState::Dependencies,
                            "Notes on Ambiguities" => state = ParsingState::Ambiguities,
                            _ => {}
                        },
                        HeadingLevel::H3 => {
                            if matches!(state, ParsingState::Requirements) {
                                if let Some((id, name)) = text.split_once(':') {
                                    current_req.id = id.trim().to_string();
                                    current_req.name = name.trim().to_string();
                                }
                            } else if matches!(state, ParsingState::OperationOrdering { .. }) {
                                match text {
                                    "State Transition Rules" => {
                                        if let ParsingState::OperationOrdering { substate } =
                                            &mut state
                                        {
                                            *substate =
                                                OperationOrderingState::StateTransitionRules;
                                        }
                                    }
                                    "Side Effect Rules" => {
                                        if let ParsingState::OperationOrdering { substate } =
                                            &mut state
                                        {
                                            *substate = OperationOrderingState::SideEffectRules;
                                        }
                                    }
                                    "Temporal Invariants" => {
                                        if let ParsingState::OperationOrdering { substate } =
                                            &mut state
                                        {
                                            *substate = OperationOrderingState::TemporalInvariants;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                    current_text.clear();
                }
                Event::Start(Tag::Strong) => {
                    if matches!(state, ParsingState::Requirements) {
                        current_text.clear();
                    }
                }
                Event::End(Tag::Strong) => {
                    if matches!(state, ParsingState::Requirements) {
                        let text = current_text.trim();
                        req_state = RequirementParsingState::from_strong_text(text);
                        if let RequirementParsingState::Dependencies = req_state {
                            current_dependency_type = None;
                        }
                        current_text.clear();
                    }
                }
                Event::Start(Tag::List(_)) => {
                    list_level += 1;
                }
                Event::End(Tag::List(_)) => {
                    list_level -= 1;
                    if list_level == 0 {
                        if let RequirementParsingState::Dependencies = req_state {
                            req_state = RequirementParsingState::Initial;
                            current_dependency_type = None;
                        }
                    }
                }
                Event::Start(Tag::Item) => {
                    current_text.clear();
                }
                Event::End(Tag::Item) => {
                    let text = current_text.trim();
                    match &state {
                        ParsingState::Requirements => match &req_state {
                            RequirementParsingState::Statement => {
                                current_req.statement = text.to_string();
                            }
                            RequirementParsingState::Category => {
                                current_req.category = text.to_string();
                            }
                            RequirementParsingState::StateChanges => {
                                current_req.state_changes = text.to_string();
                            }
                            RequirementParsingState::Ordering => {
                                parse_ordering(text, &mut current_req.ordering);
                            }
                            RequirementParsingState::Dependencies => {
                                if text.ends_with("Dependencies:") {
                                    current_dependency_type = Some(text.to_string());
                                } else {
                                    parse_dependencies(
                                        text,
                                        &current_dependency_type,
                                        &mut current_req.dependencies,
                                    );
                                }
                            }
                            RequirementParsingState::Invariants => {
                                parse_invariants(text, &mut current_req.invariants);
                            }
                            RequirementParsingState::AcceptanceCriteria => {
                                if !text.is_empty() {
                                    current_req.acceptance_criteria.push(text.to_string());
                                }
                            }
                            _ => {}
                        },
                        ParsingState::OperationOrdering { substate } => match substate {
                            OperationOrderingState::StateTransitionRules => {
                                operation_ordering
                                    .state_transition_rules
                                    .push(text.to_string());
                            }
                            OperationOrderingState::SideEffectRules => {
                                operation_ordering.side_effect_rules.push(text.to_string());
                            }
                            OperationOrderingState::TemporalInvariants => {
                                operation_ordering
                                    .temporal_invariants
                                    .push(text.to_string());
                            }
                            _ => {}
                        },
                        ParsingState::Dependencies => {
                            if let Some(dep) = parse_dependency(text) {
                                dependencies.push(dep);
                            }
                        }
                        ParsingState::Ambiguities => {
                            if let Some(amb) = parse_ambiguity(text) {
                                ambiguities.push(amb);
                            }
                        }
                        _ => {}
                    }
                    current_text.clear();
                }
                Event::Text(text) => {
                    current_text.push_str(text.as_ref());
                }
                Event::Start(Tag::Paragraph) => {
                    current_text.clear();
                }
                Event::End(Tag::Paragraph) => {
                    let text = current_text.trim();
                    if matches!(state, ParsingState::Overview) && !text.is_empty() {
                        overview = text.to_string();
                    }
                    current_text.clear();
                }
                _ => {}
            }
        }

        // Add the last requirement if any
        if !current_req.id.is_empty() {
            requirements.push(current_req);
        }

        if title.is_empty() {
            return Err(ParserError::MissingTitle);
        }

        if overview.is_empty() {
            return Err(ParserError::MissingOverview);
        }

        Ok(FunctionalRequirements {
            title,
            overview,
            requirements,
            operation_ordering,
            dependencies,
            ambiguities,
        })
    }
}

fn parse_ordering(text: &str, ordering: &mut RequirementOrdering) {
    let text = text.trim();
    if let Some(constraint) = text.strip_prefix("Must occur BEFORE:") {
        if let Some((req_id, desc)) = parse_constraint(constraint) {
            ordering
                .must_occur_before
                .push(OrderingConstraint { requirement_id: req_id, description: desc });
        }
    } else if let Some(constraint) = text.strip_prefix("Must occur AFTER:") {
        if let Some((req_id, desc)) = parse_constraint(constraint) {
            ordering
                .must_occur_after
                .push(OrderingConstraint { requirement_id: req_id, description: desc });
        }
    } else if let Some(constraint) = text.strip_prefix("Must NOT occur BEFORE:") {
        if let Some((req_id, desc)) = parse_constraint(constraint) {
            ordering
                .must_not_occur_before
                .push(OrderingConstraint { requirement_id: req_id, description: desc });
        }
    } else if let Some(constraint) = text.strip_prefix("Must NOT occur AFTER:") {
        if let Some((req_id, desc)) = parse_constraint(constraint) {
            ordering
                .must_not_occur_after
                .push(OrderingConstraint { requirement_id: req_id, description: desc });
        }
    }
}

fn parse_constraint(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    if let Some((req_id, desc)) = text.split_once('(') {
        let desc = desc.trim_end_matches(')');
        Some((req_id.trim().to_string(), desc.trim().to_string()))
    } else {
        // Handle case where there's no parentheses - treat whole text as requirement_id
        // and provide an empty description
        Some((text.trim().to_string(), String::new()))
    }
}

fn parse_dependencies(
    text: &str,
    dependency_type: &Option<String>,
    dependencies: &mut RequirementDependencies,
) {
    let text = text.trim();

    // Check if this is a dependency type line (contains a colon)
    if let Some((dep_type, content)) = text.split_once(':') {
        let dep_type = dep_type.trim();
        let content = content.trim();

        if dep_type.contains("State Dependencies") {
            dependencies.state_dependencies.push(content.to_string());
        } else if dep_type.contains("Event Dependencies") {
            dependencies.event_dependencies.push(content.to_string());
        } else if dep_type.contains("Temporal Dependencies") {
            dependencies.temporal_dependencies.push(content.to_string());
        }
        return;
    }

    // If it's just a header line with "Dependencies:" and nothing else, just return
    if text.contains("Dependencies") && !text.contains(':') {
        return;
    }

    // Handle case where we're continuing from a previously identified dependency
    // type
    match dependency_type.as_deref() {
        Some(dtype) if dtype.contains("State Dependencies") => {
            dependencies.state_dependencies.push(text.to_string());
        }
        Some(dtype) if dtype.contains("Event Dependencies") => {
            dependencies.event_dependencies.push(text.to_string());
        }
        Some(dtype) if dtype.contains("Temporal Dependencies") => {
            dependencies.temporal_dependencies.push(text.to_string());
        }
        _ => {}
    }
}

fn parse_invariants(text: &str, invariants: &mut RequirementInvariants) {
    if let Some(inv) = text.strip_prefix("State Invariants:") {
        invariants.state_invariants.push(inv.trim().to_string());
    } else if let Some(inv) = text.strip_prefix("Temporal Invariants:") {
        invariants.temporal_invariants.push(inv.trim().to_string());
    }
}

fn parse_dependency(text: &str) -> Option<Dependency> {
    // Format: **DEP-001:** REQ-NOTIFY-001 depends on REQ-STATE-001 [notifications
    // are only sent for successfully completed tasks]
    if let Some((id_part, rest)) = text.split_once(':') {
        let id = id_part.trim_start_matches('*').trim_end_matches('*').trim();
        if let Some((reqs, explanation)) = rest.trim().split_once('[') {
            if let Some((from_req, to_req)) = reqs.split_once("depends on") {
                return Some(Dependency {
                    id: id.to_string(),
                    from_req: from_req.trim().to_string(),
                    to_req: to_req.trim().to_string(),
                    explanation: explanation.trim_end_matches(']').trim().to_string(),
                });
            }
        }
    }
    None
}

fn parse_ambiguity(text: &str) -> Option<Ambiguity> {
    // Format: **AMB-001:** The specification does not detail what happens if email
    // notification fails.
    if let Some((id_part, description)) = text.split_once(':') {
        let id = id_part.trim_start_matches('*').trim_end_matches('*').trim();
        Some(Ambiguity {
            id: id.to_string(),
            description: description.trim().to_string(),
        })
    } else {
        None
    }
}

// Add is_empty methods for the types
impl RequirementOrdering {
    pub fn is_empty(&self) -> bool {
        self.must_occur_before.is_empty()
            && self.must_occur_after.is_empty()
            && self.must_not_occur_before.is_empty()
            && self.must_not_occur_after.is_empty()
    }
}

impl RequirementDependencies {
    pub fn is_empty(&self) -> bool {
        self.state_dependencies.is_empty()
            && self.event_dependencies.is_empty()
            && self.temporal_dependencies.is_empty()
    }
}

impl RequirementInvariants {
    pub fn is_empty(&self) -> bool {
        self.state_invariants.is_empty() && self.temporal_invariants.is_empty()
    }
}

impl OperationOrdering {
    pub fn is_empty(&self) -> bool {
        self.state_transition_rules.is_empty()
            && self.side_effect_rules.is_empty()
            && self.temporal_invariants.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_functional_requirements() {
        let content = r#"# Functional Requirements: Test Feature

## Overview
This is a test overview.

## Functional Requirements

### R1: First Requirement
- **Requirement:** The system shall do something.
- **Category:** TEST
- **State Changes:** None
- **Acceptance Criteria:**
  - Criterion 1
  - Criterion 2

### R2: Second Requirement
- **Requirement:** The system shall do something else.
- **Category:** TEST
- **State Changes:** None
- **Acceptance Criteria:**
  - Criterion 3
  - Criterion 4

## Dependencies
- **D1:** R2 depends on R1 [needs first thing to work]

## Notes on Ambiguities
- **AMB-001:** First ambiguity note
- **AMB-002:** Second ambiguity note"#;

        let result = FunctionalRequirements::parse(content).unwrap();
        assert_eq!(result.title, "Test Feature");
        assert_eq!(result.overview, "This is a test overview.");
        assert_eq!(result.requirements.len(), 2);
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.ambiguities.len(), 2);

        // Test first requirement
        let r1 = &result.requirements[0];
        assert_eq!(r1.id, "R1");
        assert_eq!(r1.name, "First Requirement");
        assert_eq!(r1.statement, "The system shall do something.");
        assert_eq!(r1.category, "TEST");
        assert_eq!(r1.acceptance_criteria, vec!["Criterion 1", "Criterion 2"]);

        // Test second requirement
        let r2 = &result.requirements[1];
        assert_eq!(r2.id, "R2");
        assert_eq!(r2.name, "Second Requirement");
        assert_eq!(r2.statement, "The system shall do something else.");
        assert_eq!(r2.category, "TEST");
        assert_eq!(r2.acceptance_criteria, vec!["Criterion 3", "Criterion 4"]);

        // Test dependency
        let d1 = &result.dependencies[0];
        assert_eq!(d1.id, "D1");
        let from_req = d1.from_req.trim_start_matches("** ").trim();
        assert_eq!(from_req, "R2");
        assert_eq!(d1.to_req, "R1");
        assert_eq!(d1.explanation, "needs first thing to work");
    }

    #[test]
    fn test_parse_empty_content() {
        let result = FunctionalRequirements::parse("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_sections() {
        let content = r#"# Functional Requirements: Test Feature

## Overview
This is a test overview.
"#;
        let result = FunctionalRequirements::parse(content).unwrap();
        assert_eq!(result.title, "Test Feature");
        assert_eq!(result.overview, "This is a test overview.");
        assert!(result.requirements.is_empty());
        assert!(result.dependencies.is_empty());
        assert!(result.ambiguities.is_empty());
    }

    #[test]
    fn test_parse_ordering_constraints() {
        let mut ordering = RequirementOrdering::default();
        parse_ordering(
            "Must occur BEFORE: REQ-002 (test description)",
            &mut ordering,
        );
        assert_eq!(ordering.must_occur_before.len(), 1);
        assert_eq!(ordering.must_occur_before[0].requirement_id, "REQ-002");
        assert_eq!(
            ordering.must_occur_before[0].description,
            "test description"
        );
    }

    #[test]
    fn test_parse_dependency() {
        let test_input = "**DEP-001:** REQ-002 depends on REQ-001 [test explanation]";
        let dep = parse_dependency(test_input).unwrap();

        assert_eq!(dep.id, "DEP-001");
        assert_eq!(dep.from_req, "** REQ-002");
        assert_eq!(dep.to_req, "REQ-001");
        assert_eq!(dep.explanation, "test explanation");
    }

    #[test]
    fn test_parse_ambiguity() {
        let test_input = "**AMB-001:** Test ambiguity description";
        let amb = parse_ambiguity(test_input).unwrap();

        assert_eq!(amb.id, "AMB-001");
        assert_eq!(amb.description, "** Test ambiguity description");
    }

    #[test]
    fn test_parse_todo_list_requirements() {
        let content = r#"# Functional Requirements: Todo List Application - Task Completion Feature

## Overview
This document outlines the functional requirements for the task completion feature of a command-line todo list application. The application allows technical professionals to mark tasks as complete and receive email notifications upon successful task completion.

## Functional Requirements

### REQ-STATE-001: Mark Task as Complete
- **Requirement:** The system shall update the completion status of a task to "complete" when provided with a valid task ID through the "todo done" command.
- **Category:** STATE
- **State Changes:** Task's completion status changes from incomplete to complete
- **Ordering:**
  * Must occur BEFORE: REQ-NOTIFY-001 (Email notification)
  * Must NOT occur BEFORE: Task exists in the system
- **Dependencies:**
  - State Dependencies: Task with specified ID must exist in the system
  - Event Dependencies: User must execute "todo done" command with valid task ID
- **Invariants:**
  - State Invariants: Task's existence must be verified before status change
  - Temporal Invariants: Status change must precede notification
- **Acceptance Criteria:**
  - System updates the task status to complete in the data storage
  - System preserves all other task attributes (ID, description)
  - System persists the updated status across application restarts

### REQ-NOTIFY-001: Send Email Notification on Task Completion
- **Requirement:** The system shall send an email notification containing task details when a task is successfully marked as complete.
- **Category:** NOTIFY
- **State Changes:** None (side effect only)
- **Ordering:**
  * Must occur AFTER: REQ-STATE-001 (Mark task as complete)
  * Must NOT occur BEFORE: Task status is updated to complete
  * Must NOT occur AFTER: Success message is displayed to user
- **Dependencies:**
  - State Dependencies: Task must be successfully marked as complete
  - Event Dependencies: Successful execution of "todo done" command
  - Temporal Dependencies: Must follow task status update
- **Invariants:**
  - State Invariants: Email contains current task data (ID, description)
  - Temporal Invariants: Notification occurs after status change but before user feedback
- **Acceptance Criteria:**
  - Email contains task ID, description, and timestamp
  - Email is sent to the configured recipient address
  - System attempts to ensure reliable delivery

## Operation Ordering
### State Transition Rules
- Task validation MUST complete before task status is updated
- Task status update MUST complete before email notification is sent

### Side Effect Rules
- Email notification MUST occur after task status change
- User feedback MUST follow both status change and notification attempt

### Temporal Invariants
- Task status must not change if task does not exist
- Email notification must not be sent if task status update fails

## Dependencies
- **DEP-001:** REQ-NOTIFY-001 depends on REQ-STATE-001 [notifications are only sent for successfully completed tasks]
- **DEP-002:** REQ-EVENT-002 depends on REQ-STATE-001 [feedback confirms task completion]

## Notes on Ambiguities
- **AMB-001:** The specification does not detail what happens if email notification fails.
- **AMB-002:** The specification doesn't mention if users should be able to reverse the completion status."#;

        let result = FunctionalRequirements::parse(content).unwrap();

        // Test document metadata
        assert_eq!(
            result.title,
            "Todo List Application - Task Completion Feature"
        );
        assert!(result
            .overview
            .contains("command-line todo list application"));

        // Test requirements
        assert_eq!(result.requirements.len(), 2);

        // Test REQ-STATE-001
        let req_state = &result.requirements[0];
        assert_eq!(req_state.id, "REQ-STATE-001");
        assert_eq!(req_state.name, "Mark Task as Complete");
        assert_eq!(req_state.category, "STATE");
        assert!(req_state.statement.contains("update the completion status"));
        assert!(req_state
            .state_changes
            .contains("changes from incomplete to complete"));

        // Test REQ-STATE-001 ordering - check at least one entry exists
        assert!(!req_state.ordering.must_occur_before.is_empty());

        // Test REQ-NOTIFY-001
        let req_notify = &result.requirements[1];
        assert_eq!(req_notify.id, "REQ-NOTIFY-001");
        assert_eq!(
            req_notify.name,
            "Send Email Notification on Task Completion"
        );
        assert_eq!(req_notify.category, "NOTIFY");
        assert!(req_notify.statement.contains("send an email notification"));
        assert_eq!(req_notify.state_changes, "None (side effect only)");

        // Test operation ordering
        assert!(!result.operation_ordering.state_transition_rules.is_empty());
        assert!(!result.operation_ordering.side_effect_rules.is_empty());
        assert!(!result.operation_ordering.temporal_invariants.is_empty());

        // Test dependencies
        assert!(!result.dependencies.is_empty());

        // Test ambiguities
        assert!(!result.ambiguities.is_empty());
    }

    #[test]
    fn test_serialization_empty_fields() {
        // Create a minimal requirement with empty fields
        let req = Requirement {
            id: "R1".to_string(),
            name: "Test Requirement".to_string(),
            statement: "This is a test".to_string(),
            category: "".to_string(),                         // Empty string
            state_changes: "".to_string(),                    // Empty string
            ordering: RequirementOrdering::default(),         // Empty struct
            dependencies: RequirementDependencies::default(), // Empty struct
            invariants: RequirementInvariants::default(),     // Empty struct
            acceptance_criteria: vec![],                      // Empty vector
        };

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&req).unwrap();
        println!("Serialized JSON:\n{}", json);

        // The serialized JSON should not contain empty fields
        assert!(!json.contains("category"));
        assert!(!json.contains("state_changes"));
        assert!(!json.contains("ordering"));
        assert!(!json.contains("dependencies"));
        assert!(!json.contains("invariants"));
        assert!(!json.contains("acceptance_criteria"));

        // Fields with values should be present
        assert!(json.contains("id"));
        assert!(json.contains("name"));
        assert!(json.contains("statement"));
    }
}
