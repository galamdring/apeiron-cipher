from crewai import Agent, Crew, Process, Task
from crewai.agents.agent_builder.base_agent import BaseAgent
from crewai.project import CrewBase, agent, crew, task
from pydantic import BaseModel

from apeiron_flow.config import DEFAULT_LLM
from apeiron_flow.tools.review_tools import REVIEW_TOOLS


class ReviewVerdict(BaseModel):
    """Structured output from the review task.

    verdict must be one of:
      'code_changes_required' — agent found fixable issues; REQUEST_CHANGES posted
      'human_feedback_required' — agent found ambiguous issues needing human judgement
      'ready_for_merge'        — agent approved; APPROVE posted
    """
    verdict: str
    summary: str        # top-level review summary posted to the PR
    review_url: str     # URL of the posted review, confirming it was submitted


@CrewBase
class ReviewCrew:
    """Apeiron Cipher review crew — reviews a single GitHub PR."""

    agents: list[BaseAgent]
    tasks: list[Task]

    agents_config = "config/agents.yaml"
    tasks_config = "config/tasks.yaml"

    @agent
    def code_reviewer(self) -> Agent:
        return Agent(
            config=self.agents_config["code_reviewer"],  # type: ignore[index]
            tools=REVIEW_TOOLS,
            llm=DEFAULT_LLM,
            verbose=True,
            max_iter=25,
            respect_context_window=True,
        )

    @task
    def review_pr(self) -> Task:
        return Task(
            config=self.tasks_config["review_pr"],  # type: ignore[index]
            output_pydantic=ReviewVerdict,
        )

    @crew
    def crew(self) -> Crew:
        return Crew(
            agents=self.agents,
            tasks=self.tasks,
            process=Process.sequential,
            verbose=True,
        )
