package mocks

import (
	"context"
	"encoding/json"
	"errors"

	"github.com/galamdring/apeiron-cipher/infra/orchestrator/internal/db"
)

// MockDBClient is a mock implementation of the DBClient interface for testing.
type MockDBClient struct {
	CloseFunc                 func() error
	MigrateFunc               func(ctx context.Context) error
	InsertEventFunc           func(ctx context.Context, deliveryID, eventType, action string, payload json.RawMessage) (int64, error)
	PendingJobsByWorkflowFunc func(ctx context.Context, workflowType string) ([]db.Job, error)
	NextPendingJobFunc        func(ctx context.Context) (*db.Job, error)
	ActiveJobCountFunc        func(ctx context.Context) (int, error)
	HasAnyRunningJobsFunc     func(ctx context.Context) (bool, error)
	HasRunningJobsFunc        func(ctx context.Context, workflowType string) (bool, error)
	GetEventPayloadFunc       func(ctx context.Context, eventID int64) (json.RawMessage, error)
	GetJobFunc                func(ctx context.Context, jobID int64) (*db.Job, error)
	CompleteJobFunc           func(ctx context.Context, jobID int64, status, errMsg string) error
	StartJobFunc              func(ctx context.Context, jobID int64, containerID string) error
	MarkJobLaunchingFunc      func(ctx context.Context, jobID int64, containerID string) error
	CreateJobFunc             func(ctx context.Context, eventID int64, workflowType, workerImage string) (int64, error)
	MarkEventProcessedFunc    func(ctx context.Context, eventID int64, state string) error
	ClaimPendingEventsFunc    func(ctx context.Context, limit int) ([]db.Event, error)
	UpsertPipelineConfigFunc  func(ctx context.Context, name, config string) error
	GetPipelineConfigFunc     func(ctx context.Context, name string) (string, error)
	InsertJobStepFunc         func(ctx context.Context, jobID int64, stepName string, stepIndex int) (int64, error)
	CompleteJobStepFunc       func(ctx context.Context, stepID int64, status, output, errMsg string) error
	GetJobStepsFunc           func(ctx context.Context, jobID int64) ([]db.JobStepRow, error)
	UpsertTemplateFunc        func(ctx context.Context, name, body string) error
	GetTemplateFunc           func(ctx context.Context, name string) (string, error)
}

func (m *MockDBClient) Close() error {
	if m.CloseFunc != nil {
		return m.CloseFunc()
	}
	return errors.New("CloseFunc not implemented")
}
func (m *MockDBClient) Migrate(ctx context.Context) error {
	if m.MigrateFunc != nil {
		return m.MigrateFunc(ctx)
	}
	return errors.New("MigrateFunc not implemented")
}
func (m *MockDBClient) InsertEvent(ctx context.Context, deliveryID, eventType, action string, payload json.RawMessage) (int64, error) {
	if m.InsertEventFunc != nil {
		return m.InsertEventFunc(ctx, deliveryID, eventType, action, payload)
	}
	return 0, errors.New("InsertEventFunc not implemented")
}
func (m *MockDBClient) PendingJobsByWorkflow(ctx context.Context, workflowType string) ([]db.Job, error) {
	if m.PendingJobsByWorkflowFunc != nil {
		return m.PendingJobsByWorkflowFunc(ctx, workflowType)
	}
	return nil, errors.New("PendingJobsByWorkflowFunc not implemented")
}
func (m *MockDBClient) NextPendingJob(ctx context.Context) (*db.Job, error) {
	if m.NextPendingJobFunc != nil {
		return m.NextPendingJobFunc(ctx)
	}
	return nil, errors.New("NextPendingJobFunc not implemented")
}
func (m *MockDBClient) ActiveJobCount(ctx context.Context) (int, error) {
	if m.ActiveJobCountFunc != nil {
		return m.ActiveJobCountFunc(ctx)
	}
	return 0, errors.New("ActiveJobCountFunc not implemented")
}
func (m *MockDBClient) HasAnyRunningJobs(ctx context.Context) (bool, error) {
	if m.HasAnyRunningJobsFunc != nil {
		return m.HasAnyRunningJobsFunc(ctx)
	}
	return false, errors.New("HasAnyRunningJobsFunc not implemented")
}
func (m *MockDBClient) HasRunningJobs(ctx context.Context, workflowType string) (bool, error) {
	if m.HasRunningJobsFunc != nil {
		return m.HasRunningJobsFunc(ctx, workflowType)
	}
	return false, errors.New("HasRunningJobsFunc not implemented")
}
func (m *MockDBClient) GetEventPayload(ctx context.Context, eventID int64) (json.RawMessage, error) {
	if m.GetEventPayloadFunc != nil {
		return m.GetEventPayloadFunc(ctx, eventID)
	}
	return nil, errors.New("GetEventPayloadFunc not implemented")
}
func (m *MockDBClient) GetJob(ctx context.Context, jobID int64) (*db.Job, error) {
	if m.GetJobFunc != nil {
		return m.GetJobFunc(ctx, jobID)
	}
	return nil, errors.New("GetJobFunc not implemented")
}
func (m *MockDBClient) CompleteJob(ctx context.Context, jobID int64, status, errMsg string) error {
	if m.CompleteJobFunc != nil {
		return m.CompleteJobFunc(ctx, jobID, status, errMsg)
	}
	return errors.New("CompleteJobFunc not implemented")
}
func (m *MockDBClient) StartJob(ctx context.Context, jobID int64, containerID string) error {
	if m.StartJobFunc != nil {
		return m.StartJobFunc(ctx, jobID, containerID)
	}
	return errors.New("StartJobFunc not implemented")
}
func (m *MockDBClient) MarkJobLaunching(ctx context.Context, jobID int64, containerID string) error {
	if m.MarkJobLaunchingFunc != nil {
		return m.MarkJobLaunchingFunc(ctx, jobID, containerID)
	}
	return errors.New("MarkJobLaunchingFunc not implemented")
}
func (m *MockDBClient) CreateJob(ctx context.Context, eventID int64, workflowType, workerImage string) (int64, error) {
	if m.CreateJobFunc != nil {
		return m.CreateJobFunc(ctx, eventID, workflowType, workerImage)
	}
	return 0, errors.New("CreateJobFunc not implemented")
}
func (m *MockDBClient) MarkEventProcessed(ctx context.Context, eventID int64, state string) error {
	if m.MarkEventProcessedFunc != nil {
		return m.MarkEventProcessedFunc(ctx, eventID, state)
	}
	return errors.New("MarkEventProcessedFunc not implemented")
}
func (m *MockDBClient) ClaimPendingEvents(ctx context.Context, limit int) ([]db.Event, error) {
	if m.ClaimPendingEventsFunc != nil {
		return m.ClaimPendingEventsFunc(ctx, limit)
	}
	return nil, errors.New("ClaimPendingEventsFunc not implemented")
}
func (m *MockDBClient) UpsertPipelineConfig(ctx context.Context, name, config string) error {
	if m.UpsertPipelineConfigFunc != nil {
		return m.UpsertPipelineConfigFunc(ctx, name, config)
	}
	return errors.New("UpsertPipelineConfigFunc not implemented")
}
func (m *MockDBClient) GetPipelineConfig(ctx context.Context, name string) (string, error) {
	if m.GetPipelineConfigFunc != nil {
		return m.GetPipelineConfigFunc(ctx, name)
	}
	return "", errors.New("GetPipelineConfigFunc not implemented")
}
func (m *MockDBClient) InsertJobStep(ctx context.Context, jobID int64, stepName string, stepIndex int) (int64, error) {
	if m.InsertJobStepFunc != nil {
		return m.InsertJobStepFunc(ctx, jobID, stepName, stepIndex)
	}
	return 0, errors.New("InsertJobStepFunc not implemented")
}
func (m *MockDBClient) CompleteJobStep(ctx context.Context, stepID int64, status, output, errMsg string) error {
	if m.CompleteJobStepFunc != nil {
		return m.CompleteJobStepFunc(ctx, stepID, status, output, errMsg)
	}
	return errors.New("CompleteJobStepFunc not implemented")
}
func (m *MockDBClient) GetJobSteps(ctx context.Context, jobID int64) ([]db.JobStepRow, error) {
	if m.GetJobStepsFunc != nil {
		return m.GetJobStepsFunc(ctx, jobID)
	}
	return nil, errors.New("GetJobStepsFunc not implemented")
}
func (m *MockDBClient) UpsertTemplate(ctx context.Context, name, body string) error {
	if m.UpsertTemplateFunc != nil {
		return m.UpsertTemplateFunc(ctx, name, body)
	}
	return errors.New("UpsertTemplateFunc not implemented")
}
func (m *MockDBClient) GetTemplate(ctx context.Context, name string) (string, error) {
	if m.GetTemplateFunc != nil {
		return m.GetTemplateFunc(ctx, name)
	}
	return "", errors.New("GetTemplateFunc not implemented")
}
