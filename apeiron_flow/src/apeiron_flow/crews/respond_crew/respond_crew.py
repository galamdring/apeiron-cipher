from crewai import Agent, Crew, Process, Task
from crewai.agents.agent_builder.base_agent import BaseAgent
from crewai.project import CrewBase, agent, crew, task
from pydantic import BaseModel

from apeiron_flow.config import DEFAULT_LLM
from apeiron_flow.tools.respond_tools import RESPOND_TOOLS


class RespondResult(BaseModel):
    """Structured output from the respond task."""
    intent: str        # change_request | question | approval | out_of_scope
    reply: str         # text of the comment posted to GitHub
    comment_url: str   # URL of the posted comment


@CrewBase
class RespondCrew:
    """Apeiron Cipher respond crew — handles a single @mention."""

    agents: list[BaseAgent]
    tasks: list[Task]

    agents_config = "config/agents.yaml"
    tasks_config = "config/tasks.yaml"

    @agent
    def responder(self) -> Agent:
        return Agent(
            config=self.agents_config["responder"],  # type: ignore[index]
            tools=RESPOND_TOOLS,
            llm=DEFAULT_LLM,
            verbose=True,
            max_iter=30,
            respect_context_window=True,
        )

    @task
    def respond_to_mention(self) -> Task:
        return Task(
            config=self.tasks_config["respond_to_mention"],  # type: ignore[index]
            output_pydantic=RespondResult,
        )

    @crew
    def crew(self) -> Crew:
        return Crew(
            agents=self.agents,
            tasks=self.tasks,
            process=Process.sequential,
            verbose=True,
        )
