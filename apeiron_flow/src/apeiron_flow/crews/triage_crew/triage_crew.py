"""
TriageCrew — reads a status:triage issue, decomposes it, and transitions labels.
"""

from crewai import Agent, Crew, Process, Task
from crewai.agents.agent_builder.base_agent import BaseAgent
from crewai.project import CrewBase, agent, crew, task
from pydantic import BaseModel

from apeiron_flow.config import DEFAULT_LLM
from apeiron_flow.tools.triage_tools import (
    create_child_issue,
    get_issue_details,
    get_repo_labels,
    list_child_issues,
    post_issue_comment,
    set_status_label,
)


class TriageResult(BaseModel):
    """Structured output returned by the TriageCrew."""

    classification: str
    """One of: epic, story_decomposed, story_atomic, blocked_ambiguous."""

    child_issues: list[int]
    """Issue numbers of newly-created child issues. Empty for atomic stories."""

    comment: str
    """Human-readable summary of the triage action (or clarifying question asked)."""


@CrewBase
class TriageCrew:
    """Triage crew — classifies and decomposes a status:triage GitHub issue."""

    agents: list[BaseAgent]
    tasks: list[Task]

    agents_config = "config/agents.yaml"
    tasks_config = "config/tasks.yaml"

    @agent
    def issue_analyst(self) -> Agent:
        return Agent(
            config=self.agents_config["issue_analyst"],  # type: ignore[index]
            llm=DEFAULT_LLM,
            tools=[
                get_issue_details,
                list_child_issues,
                create_child_issue,
                set_status_label,
                post_issue_comment,
                get_repo_labels,
            ],
            verbose=True,
        )

    @task
    def triage_issue(self) -> Task:
        return Task(
            config=self.tasks_config["triage_issue"],  # type: ignore[index]
            output_pydantic=TriageResult,
        )

    @crew
    def crew(self) -> Crew:
        return Crew(
            agents=self.agents,
            tasks=self.tasks,
            process=Process.sequential,
            verbose=True,
        )
