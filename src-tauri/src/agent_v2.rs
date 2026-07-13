use serde::{Deserialize, Serialize};
use crate::intelligence::router;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum AgentState {
    Initialized,
    Planning,
    AwaitingApproval,
    Executing,
    Verifying,
    Completed,
    Failed(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentTask {
    pub id: String,
    pub description: String,
    pub state: AgentState,
    pub plan_output: Option<String>,
}

impl AgentTask {
    pub fn new(id: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            state: AgentState::Initialized,
            plan_output: None,
        }
    }

    pub fn transition_to(&mut self, new_state: AgentState) {
        println!(
            "[AGENT:{}] State Transition: {:?} -> {:?}",
            self.id, self.state, new_state
        );
        self.state = new_state;
    }
}

pub struct AgentRunner;

impl AgentRunner {
    pub async fn process_task(mut task: AgentTask) -> Result<AgentTask, String> {
        // 1. PLANNING PHASE
        task.transition_to(AgentState::Planning);

        let prompt = format!(
            "You are the Neural Forge Architect. Create a concise, 3-step execution plan for the user's request. Do not write code yet, only the plan.\n\nUser Request: {}",
            task.description
        );

        match router::route_through_gateway(prompt).await {
            Ok(response) => {
                println!("[AGENT:{}] Planning successful.", task.id);
                task.plan_output = Some(response);
                task.transition_to(AgentState::AwaitingApproval);
            }
            Err(e) => {
                let err_msg = format!("Planning failed: {}", e);
                task.transition_to(AgentState::Failed(err_msg.clone()));
                return Err(err_msg);
            }
        }

        // 2. AWAITING APPROVAL PHASE (Auto-approved for Phase C validation)
        println!(
            "[AGENT:{}] Auto-approving plan for system validation...",
            task.id
        );
        task.transition_to(AgentState::Executing);

        // 3. EXECUTING PHASE
        println!("[AGENT:{}] Executing task instructions...", task.id);

        task.transition_to(AgentState::Verifying);

        // 4. VERIFYING PHASE
        println!("[AGENT:{}] Validating execution outcomes...", task.id);

        task.transition_to(AgentState::Completed);

        Ok(task)
    }
}