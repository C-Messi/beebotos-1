//! DAPO — Dynamic Sampling Policy Optimization
//!
//! Maintains exploration-exploitation balance by dynamically adjusting
//! the sampling temperature based on policy entropy monitoring.
//! Prevents entropy collapse in RL-trained agent strategies.

use std::collections::VecDeque;

/// DAPO training configuration
#[derive(Debug, Clone)]
pub struct DapoConfig {
    /// Initial sampling temperature
    pub initial_temperature: f32,
    /// Temperature decay factor when entropy is too high
    pub temperature_decay: f32,
    /// Minimum temperature (prevent pure random)
    pub min_temperature: f32,
    /// Target entropy level to maintain
    pub target_entropy: f32,
    /// Entropy collapse detection threshold
    pub entropy_collapse_threshold: f32,
    /// Recovery sampling ratio when collapse detected
    pub recovery_sampling_ratio: f32,
    /// Entropy coefficient for policy loss
    pub entropy_coefficient: f32,
    /// Maximum temperature cap
    pub max_temperature: f32,
}

impl Default for DapoConfig {
    fn default() -> Self {
        Self {
            initial_temperature: 1.0,
            temperature_decay: 0.95,
            min_temperature: 0.1,
            target_entropy: 1.5,
            entropy_collapse_threshold: 0.3,
            recovery_sampling_ratio: 2.0,
            entropy_coefficient: 0.01,
            max_temperature: 2.0,
        }
    }
}

/// Training metrics from a DAPO step
#[derive(Debug, Clone)]
pub struct TrainingMetrics {
    pub policy_loss: f32,
    pub value_loss: f32,
    pub entropy: f32,
    pub temperature: f32,
    pub kl_divergence: f32,
}

/// Temperature scheduler with entropy-aware dynamic adjustment
#[derive(Debug, Clone)]
pub struct TemperatureScheduler {
    config: DapoConfig,
    entropy_history: VecDeque<f32>,
    current_temperature: f32,
}

impl TemperatureScheduler {
    pub fn new(config: DapoConfig) -> Self {
        Self {
            current_temperature: config.initial_temperature,
            entropy_history: VecDeque::new(),
            config,
        }
    }

    /// Update temperature based on current policy entropy
    pub fn update(&mut self, current_entropy: f32) -> f32 {
        self.entropy_history.push_back(current_entropy);
        if self.entropy_history.len() > 100 {
            self.entropy_history.pop_front();
        }

        let avg_entropy: f32 = self.entropy_history.iter().sum::<f32>()
            / self.entropy_history.len().max(1) as f32;

        if avg_entropy < self.config.entropy_collapse_threshold {
            // Entropy collapse detected → boost exploration
            let boost = (self.config.target_entropy - avg_entropy)
                * self.config.recovery_sampling_ratio;
            self.current_temperature = (self.current_temperature + boost)
                .min(self.config.max_temperature);

            tracing::warn!(
                "DAPO entropy collapse detected: {:.3} → boosting temperature to {:.3}",
                avg_entropy, self.current_temperature
            );
        } else if avg_entropy > self.config.target_entropy * 1.2 {
            // Entropy too high → reduce exploration
            self.current_temperature *= self.config.temperature_decay;
            self.current_temperature = self.current_temperature.max(self.config.min_temperature);
        }

        self.current_temperature
    }

    /// Apply temperature to logits for sampling
    pub fn apply_temperature(&self, logits: &[f32]) -> Vec<f32> {
        logits.iter().map(|&l| l / self.current_temperature).collect()
    }

    pub fn current_temperature(&self) -> f32 {
        self.current_temperature
    }
}

/// A batch of trajectories for training
#[derive(Debug, Clone)]
pub struct TrajectoryBatch {
    pub trajectories: Vec<Trajectory>,
}

/// Single trajectory with actions and rewards
#[derive(Debug, Clone)]
pub struct Trajectory {
    pub states: Vec<String>,
    pub actions: Vec<usize>,
    pub rewards: Vec<f32>,
    pub final_reward: f32,
}

/// Simple policy representation (action probabilities per state)
#[derive(Debug, Clone)]
pub struct Policy {
    pub action_probs: Vec<Vec<f32>>,
}

impl Policy {
    /// Compute entropy of the policy
    pub fn entropy(&self) -> f32 {
        let mut total_entropy = 0.0;
        for probs in &self.action_probs {
            let mut h = 0.0;
            for &p in probs {
                if p > 1e-10 {
                    h -= p * p.ln();
                }
            }
            total_entropy += h;
        }
        total_entropy / self.action_probs.len().max(1) as f32
    }

    /// Compute KL divergence from another policy
    pub fn kl_divergence(&self, other: &Policy) -> f32 {
        let mut total_kl = 0.0;
        for (p_probs, q_probs) in self.action_probs.iter().zip(other.action_probs.iter()) {
            let mut kl = 0.0;
            for (&p, &q) in p_probs.iter().zip(q_probs.iter()) {
                if p > 1e-10 && q > 1e-10 {
                    kl += p * (p / q).ln();
                }
            }
            total_kl += kl;
        }
        total_kl / self.action_probs.len().max(1) as f32
    }
}

/// DAPO trainer
#[derive(Debug, Clone)]
pub struct DapoTrainer {
    config: DapoConfig,
    temperature_scheduler: TemperatureScheduler,
    /// Current policy (simplified: action logits)
    current_policy: Option<Policy>,
    /// Baseline policy for KL divergence
    baseline_policy: Option<Policy>,
}

impl DapoTrainer {
    pub fn new(config: DapoConfig) -> Self {
        let scheduler = TemperatureScheduler::new(config.clone());
        Self {
            config,
            temperature_scheduler: scheduler,
            current_policy: None,
            baseline_policy: None,
        }
    }

    /// Compute policy entropy from a batch
    pub fn compute_entropy(&self, batch: &TrajectoryBatch) -> f32 {
        // Simplified: estimate entropy from action diversity
        let mut action_counts: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        let mut total_actions = 0usize;

        for traj in &batch.trajectories {
            for &action in &traj.actions {
                *action_counts.entry(action).or_insert(0) += 1;
                total_actions += 1;
            }
        }

        if total_actions == 0 {
            return 0.0;
        }

        let mut entropy = 0.0;
        for &count in action_counts.values() {
            let p = count as f32 / total_actions as f32;
            if p > 1e-10 {
                entropy -= p * p.ln();
            }
        }
        entropy
    }

    /// Sample actions with temperature-adjusted logits
    pub fn sample_with_temperature(
        &self,
        logits: &[f32],
        temperature: f32,
    ) -> Vec<f32> {
        let scaled: Vec<f32> = logits.iter().map(|&l| l / temperature).collect();
        let max_logit = scaled.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let exp_sum: f32 = scaled.iter().map(|&l| (l - max_logit).exp()).sum();
        scaled.iter().map(|&l| (l - max_logit).exp() / exp_sum).collect()
    }

    /// Compute advantages (simplified: reward-to-go baseline)
    pub fn compute_advantages(&self, batch: &TrajectoryBatch) -> Vec<Vec<f32>> {
        batch.trajectories.iter().map(|traj| {
            let n = traj.rewards.len();
            let mut advantages = Vec::with_capacity(n);
            let mut running_sum = 0.0;
            for i in (0..n).rev() {
                running_sum += traj.rewards[i];
                advantages.push(running_sum);
            }
            advantages.reverse();
            advantages
        }).collect()
    }

    /// Update policy (simplified CLIP-like update)
    pub fn update_policy(
        &mut self,
        batch: &TrajectoryBatch,
        _advantages: &[Vec<f32>],
        entropy: f32,
    ) -> f32 {
        // Placeholder: in a real implementation this would call into an ML framework
        let entropy_bonus = self.entropy_bonus(entropy);
        let loss = -entropy_bonus; // maximize entropy bonus = minimize negative

        // Update current policy placeholder
        self.current_policy = Some(self.estimate_policy(batch));

        loss
    }

    /// Update value function (placeholder)
    pub fn update_critic(&mut self, batch: &TrajectoryBatch) -> f32 {
        let total_reward: f32 = batch.trajectories.iter()
            .map(|t| t.rewards.iter().sum::<f32>())
            .sum();
        total_reward / batch.trajectories.len().max(1) as f32
    }

    /// Dynamic entropy bonus: higher when entropy is below target
    pub fn entropy_bonus(&self, entropy: f32) -> f32 {
        let deficit = (self.config.target_entropy - entropy).max(0.0);
        self.config.entropy_coefficient * (1.0 + deficit * 2.0)
    }

    /// Estimate a simple policy from trajectory action frequencies
    fn estimate_policy(&self, batch: &TrajectoryBatch) -> Policy {
        let mut action_probs = Vec::new();
        for traj in &batch.trajectories {
            let mut counts: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
            for &a in &traj.actions {
                *counts.entry(a).or_insert(0) += 1;
            }
            let total = traj.actions.len().max(1);
            let max_action = counts.keys().copied().max().unwrap_or(0) + 1;
            let mut probs = vec![0.0f32; max_action];
            for (action, count) in counts {
                probs[action] = count as f32 / total as f32;
            }
            action_probs.push(probs);
        }
        Policy { action_probs }
    }

    /// Single training step
    pub fn train_step(&mut self, batch: TrajectoryBatch) -> TrainingMetrics {
        // 1. Compute current entropy
        let current_entropy = self.compute_entropy(&batch);

        // 2. Update temperature
        let temperature = self.temperature_scheduler.update(current_entropy);

        // 3. Sample with temperature (conceptual)
        if let Some(ref policy) = self.current_policy {
            for probs in &policy.action_probs {
                let _ = self.sample_with_temperature(probs, temperature);
            }
        }

        // 4. Compute advantages
        let advantages = self.compute_advantages(&batch);

        // 5. Update policy
        let policy_loss = self.update_policy(&batch, &advantages, current_entropy);

        // 6. Update critic
        let value_loss = self.update_critic(&batch);

        // 7. KL divergence
        let kl = if let (Some(ref current), Some(ref baseline)) = (&self.current_policy, &self.baseline_policy) {
            current.kl_divergence(baseline)
        } else {
            0.0
        };

        // Update baseline
        if self.current_policy.is_some() {
            self.baseline_policy = self.current_policy.clone();
        }

        TrainingMetrics {
            policy_loss,
            value_loss,
            entropy: current_entropy,
            temperature,
            kl_divergence: kl,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_scheduler_collapse() {
        let config = DapoConfig {
            initial_temperature: 1.0,
            target_entropy: 1.5,
            entropy_collapse_threshold: 0.5,
            recovery_sampling_ratio: 2.0,
            max_temperature: 2.0,
            ..Default::default()
        };
        let mut scheduler = TemperatureScheduler::new(config);

        // Simulate entropy collapse
        for _ in 0..10 {
            scheduler.update(0.1);
        }

        assert!(scheduler.current_temperature() > 1.0, "Temperature should boost after collapse");
    }

    #[test]
    fn test_temperature_scheduler_decay() {
        let config = DapoConfig {
            initial_temperature: 1.5,
            target_entropy: 1.0,
            temperature_decay: 0.9,
            min_temperature: 0.1,
            ..Default::default()
        };
        let mut scheduler = TemperatureScheduler::new(config);

        // High entropy → decay
        for _ in 0..10 {
            scheduler.update(2.0);
        }

        assert!(scheduler.current_temperature() < 1.5, "Temperature should decay with high entropy");
    }

    #[test]
    fn test_policy_entropy() {
        let policy = Policy {
            action_probs: vec![
                vec![0.5, 0.5],
                vec![0.9, 0.1],
            ],
        };
        let entropy = policy.entropy();
        assert!(entropy > 0.0);
    }

    #[test]
    fn test_dapo_train_step() {
        let mut trainer = DapoTrainer::new(DapoConfig::default());
        let batch = TrajectoryBatch {
            trajectories: vec![
                Trajectory {
                    states: vec!["s1".to_string(), "s2".to_string()],
                    actions: vec![0, 1],
                    rewards: vec![0.5, 1.0],
                    final_reward: 1.5,
                },
            ],
        };

        let metrics = trainer.train_step(batch);
        assert!(metrics.entropy >= 0.0);
        assert!(metrics.temperature > 0.0);
    }

    #[test]
    fn test_entropy_bonus() {
        let trainer = DapoTrainer::new(DapoConfig::default());
        let low_bonus = trainer.entropy_bonus(2.0); // above target
        let high_bonus = trainer.entropy_bonus(0.1); // far below target
        assert!(high_bonus > low_bonus, "Low entropy should get higher bonus");
    }
}
