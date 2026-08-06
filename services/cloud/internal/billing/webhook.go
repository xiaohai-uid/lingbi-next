package billing

import "sync"

type EntitlementMutator interface {
	Apply(event BillingEvent) error
}

type MemoryEntitlementMutator struct {
	mu      sync.Mutex
	applied map[string]int
}

func NewMemoryEntitlementMutator() *MemoryEntitlementMutator {
	return &MemoryEntitlementMutator{applied: make(map[string]int)}
}

func (m *MemoryEntitlementMutator) Apply(event BillingEvent) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.applied[event.ID]++
	return nil
}

func (m *MemoryEntitlementMutator) Count(eventID string) int {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.applied[eventID]
}

type WebhookService struct {
	mu      sync.Mutex
	seen    map[string]struct{}
	mutator EntitlementMutator
}

func NewWebhookService(mutator EntitlementMutator) *WebhookService {
	return &WebhookService{
		seen:    make(map[string]struct{}),
		mutator: mutator,
	}
}

func (s *WebhookService) Handle(event BillingEvent) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, exists := s.seen[event.ID]; exists {
		return nil
	}
	s.seen[event.ID] = struct{}{}
	if err := s.mutator.Apply(event); err != nil {
		delete(s.seen, event.ID)
		return err
	}
	return nil
}
