from crewai import Agent, Crew, Process, Task
from crewai.agents.agent_builder.base_agent import BaseAgent
from crewai.project import CrewBase, agent, crew, task

from apeiron_flow.config import DEFAULT_LLM
from apeiron_flow.tools.dev_tools import ALL_TOOLS


@CrewBase
class DevCrew:
    """Apeiron Cipher developer crew — implements a single GitHub issue."""

    agents: list[BaseAgent]
    tasks: list[Task]

    agents_config = "config/agents.yaml"
    tasks_config = "config/tasks.yaml"

    @agent
    def rust_developer(self) -> Agent:
        return Agent(
            config=self.agents_config["rust_developer"],  # type: ignore[index]
            tools=ALL_TOOLS,
            llm=DEFAULT_LLM,
            verbose=True,
            max_iter=75,
            respect_context_window=True,
        )

    @task
    def implement_issue(self) -> Task:
        return Task(
            config=self.tasks_config["implement_issue"],  # type: ignore[index]
        )

    @crew
    def crew(self) -> Crew:
        return Crew(
            agents=self.agents,
            tasks=self.tasks,
            process=Process.sequential,
            verbose=True,
        )
