# language: en
# IDBD Rust crate and visual demo — BDD acceptance plan
# Status: proposed behavior; Rust and browser step definitions are not implemented.
# Date: 2026-11-02
#
# Scope:
#   v0.1 core: dense f64 inputs, scalar linear prediction, explicit configuration,
#   checked atomic updates, optional bounds, inspection, and full in-memory state.
#   Demo milestone: the same core compiled to WebAssembly through a separate
#   binding crate; interactive browser visuals and a fixed-step LMS comparison.
#   Later: sparse inputs, optional serialization, and a separate Autostep learner.
#
# Definitions:
#   alpha = exp(beta); theta = meta step size; h = adaptation trace.
#   Error/residual = target minus prediction, measured before the update.
#   "Standard learner" means 2 features, alpha 0.1, theta 0.01, no bounds,
#   and zero initial weights and traces.
#   "Unchanged state" means bitwise-identical committed config, weights, betas,
#   and traces. Internal scratch memory is excluded.
#   Equation comparisons use abs(a-b) <= 1e-15 + 1e-13 * max(abs(a), abs(b)).
#   Snapshot copy and identical same-build replay use bitwise comparisons.
#   Native/WASM fixtures use numerical tolerance; cross-platform bitwise
#   agreement and arbitrary-stream stability are not promised.
#   Every error scenario contains one independent fault unless stated otherwise.
#
# Bounds contract:
#   Optional fields: finite minimum beta, finite maximum beta, and finite
#   strictly positive maximum absolute beta increment.
#   The beta interval may have equal endpoints; minimum must not exceed maximum.
#   Initialization must satisfy configured bounds; inconsistent input is rejected.
#   First compute and validate the raw beta increment and unbounded beta proposal.
#   Then clip the increment, add it to old beta, and apply beta floor/ceiling.
#   Optional safety configuration inspired by Sutton: minimum beta -10, maximum increment 2, no beta ceiling. Paper reproduction disables these bounds.
#   Reject non-finite raw arithmetic before applying bounds.
#   bounded_features counts each coordinate once when its final bounded beta
#   differs from its raw unbounded beta proposal in floating-point arithmetic.
#   Merely touching a bound counts zero; failed updates return no Update report.
#
# Test strategy:
#   Implement core scenarios as ordinary Rust tests first, preserving scenario
#   IDs in test names. A Cucumber runner is optional development tooling.
#   Experiment and performance scenarios run separately from fast core tests.
#   Paper-reproduction fixtures have independent calculations and recorded provenance.
#   Browser scenarios exercise the built WASM module, not a JS copy of IDBD.
#
# Sources:
#   Sutton 1992, Figure 2 and pp. 172-174:
#   https://cdn.aaai.org/AAAI/1992/AAAI92-027.pdf
#   Mahmood et al. 2012, Autostep Table 1:
#   https://armahmood.github.io/files/MSDP-Autostep-ICASSP-2012.pdf
#   Gherkin syntax:
#   https://cucumber.io/docs/gherkin/reference/
#   Rust/browser binding:
#   https://rustwasm.github.io/docs/wasm-bindgen/

@planned
Feature: Dependable online learning with IDBD and an interactive Rust demo
  As a Rust developer working with changing data
  I want a small learner with observable and reproducible behavior
  So that I can evaluate it, use it in an application, and understand its updates

  @v0_1 @core @fast
  Rule: Construction establishes a valid and explicit learning state

    Scenario: C01 Initialize a learner
      Given 3 features, initial alpha 0.05, theta 0.01 and no bounds
      When I construct the learner
      Then its weights and traces are zero
      And every beta equals ln(0.05) within the equation tolerance
      And every alpha is finite and strictly positive
      And its feature count and configuration match the request

    Scenario Outline: C02 Reject invalid configuration
      Given otherwise valid configuration
      And the configuration fault is "<fault>"
      When I construct the learner
      Then construction returns an invalid-configuration error
      And no learner is produced

      Examples:
        | fault                                      |
        | zero features                              |
        | initial alpha is zero                      |
        | initial alpha is negative                  |
        | initial alpha is NaN                       |
        | initial alpha is positive infinity         |
        | initial alpha is negative infinity         |
        | theta is negative                          |
        | theta is NaN                               |
        | theta is positive infinity                 |
        | theta is negative infinity                 |
        | a configured bound is non-finite           |
        | maximum beta increment is zero             |
        | maximum beta increment is negative         |
        | minimum beta exceeds maximum beta          |
        | initial log alpha is below minimum beta    |
        | initial log alpha is above maximum beta    |

  @v0_1 @core @fast
  Rule: Prediction and inspection describe committed state without mutation

    Scenario: C03 Predict a known linear result
      Given a valid restored learner with weights [2.0, -1.0, 0.5]
      When I predict for features [3.0, 4.0, 2.0]
      Then the prediction is 3.0 within the equation tolerance
      And the committed state is unchanged

    Scenario: C04 Inspect the state without copying it into new vectors
      Given a standard learner after one successful update
      When I inspect its weights, betas, traces, feature count and configuration
      And I consume the step-size iterator without collecting it
      Then the values describe the committed state
      And every reported step size equals exp of its corresponding beta
      And the committed state is unchanged

    Scenario Outline: C05 Reject an invalid feature vector
      Given a valid learner with 3 features
      And a feature vector with "<fault>"
      When I call "<operation>"
      Then I receive the corresponding input error
      And the committed state is unchanged

      Examples:
        | fault                     | operation                  |
        | only 2 elements           | predict                    |
        | 4 elements                | predict                    |
        | only 2 elements           | update with finite target  |
        | 4 elements                | update with finite target  |
        | a NaN element             | predict                    |
        | a positive infinity       | predict                    |
        | a negative infinity       | predict                    |
        | a NaN element             | update with finite target  |
        | a positive infinity       | update with finite target  |
        | a negative infinity       | update with finite target  |

  @v0_1 @core @fast
  Rule: Every update follows the IDBD recurrence

    Scenario: C06 Learn the first example from zero traces
      Given a standard learner
      When I update with features [1.0, 2.0] and target 1.0
      Then the report contains pre-update prediction 0.0 and error 1.0
      And the weights are [0.1, 0.2] within the equation tolerance
      And the traces are [0.1, 0.2] within the equation tolerance
      And both betas remain unchanged
      And bounded_features is 0

    Scenario: C07 Use old traces and one shared pre-update error
      Given the learner state produced by C06
      When I update with features [2.0, -1.0] and target 0.5
      Then the report contains pre-update prediction 0.0 and error 0.5 within tolerance
      And the resulting state matches this independently calculated fixture:
        | feature | beta                   | alpha               | weight              | trace               |
        | 0       | ln(0.1) + 0.001        | 0.10010005001667084 | 0.20010005001667086 | 0.16006003001000252 |
        | 1       | ln(0.1) - 0.001        | 0.09990004998333751 | 0.15004997500833125 | 0.13006996501166376 |

    Scenario: C08 Disable meta learning explicitly
      Given a learner with theta 0 and no bounds
      And an independent fixed-step LMS learner with matching weights and step sizes
      When both process the same finite deterministic sequence
      Then IDBD betas remain unchanged
      And their pre-update predictions, errors and updated weights agree within tolerance

    Scenario: C09 Preserve an inactive coordinate
      Given a valid learner with nonzero weights and traces
      When a successful update has feature 1 equal to zero
      Then weight 1, beta 1 and trace 1 are unchanged

    Scenario: C10 Decay the trace even when the prediction is already correct
      Given a 1-feature learner with alpha 0.5, theta 0.01 and no bounds
      And it has learned features [1.0] with target 2.0
      When I update with features [1.0] and target 1.0
      Then the pre-update error is zero
      And the weight remains 1.0
      And beta remains unchanged
      And the trace becomes 0.5

    Scenario: C11 Clamp negative trace decay to zero
      Given a 1-feature learner with alpha 0.5, theta 0.01 and no bounds
      And it has learned features [1.0] with target 2.0
      When I update with features [2.0] and target 2.0
      Then the pre-update error is zero
      And the weight remains 1.0
      And beta remains unchanged
      And the trace becomes zero

  @v0_1 @core @fast
  Rule: Bounds are explicit and their activity is observable

    Scenario Outline: C12 Apply the optional safety bounds in the specified order
      Given a valid 1-feature state using the optional safety bounds
      And its old beta is <old_beta>
      And the finite example and old trace produce raw beta increment <increment>
      When I update the learner successfully
      Then its final beta is <final_beta> within the equation tolerance
      And bounded_features is <count>

      Examples:
        | old_beta | increment | final_beta | count |
        | -5       | 3         | -3         | 1     |
        | -5       | -3        | -7         | 1     |
        | -9       | -3        | -10        | 1     |
        | -10      | -1        | -10        | 1     |
        | -9       | -1        | -10        | 0     |
        | -5       | 2         | -3         | 0     |

    Scenario: C13 Apply an optional beta ceiling
      Given custom bounds with maximum beta 0 and no increment cap
      And a valid state with old beta -1
      And a finite example and old trace produce raw beta increment 2
      When I update the learner successfully
      Then its final beta is 0
      And its step size is 1
      And bounded_features is 1

    Scenario: C14 Preserve the original equations when bounds are disabled
      Given two valid learners with the same weights, betas and traces
      And their configurations differ only in using no bounds or optional safety bounds
      And a finite example produces a raw beta increment of 3 for one feature
      When both update successfully
      Then the unbounded learner applies the full increment
      And the bounded learner applies an increment of 2 to that feature
      And both still perform all numerical validity checks

  @v0_1 @core @fast
  Rule: Failed operations preserve all committed learning state

    Scenario Outline: C15 Reject a non-finite target
      Given a standard learner and valid features
      When I update with target "<target>"
      Then I receive a non-finite-target error
      And the committed state is unchanged
      And no Update report is returned

      Examples:
        | target            |
        | NaN               |
        | positive infinity |
        | negative infinity |

    Scenario: C16 Reject an overflowing prediction
      Given valid finite state and finite features whose dot product overflows
      When I predict
      Then I receive a numerical error identifying the prediction stage
      And the committed state is unchanged

    Scenario: C17 Reject finite inputs whose residual overflows
      Given a valid learner with finite prediction and finite target
      And subtracting the prediction from the target overflows
      When I update
      Then I receive a numerical error identifying the residual stage
      And the committed state is unchanged

    Scenario: C18 Reject a late failure without committing earlier coordinates
      Given a valid multi-feature learner and a snapshot of its state
      And finite inputs allow the first coordinate candidate to succeed
      And the last coordinate candidate has a non-finite weight or trace
      When I update
      Then I receive a numerical error identifying the failed stage and coordinate
      And every committed coordinate matches the original snapshot bit for bit
      And no Update report is returned

    Scenario Outline: C19 Reject an unrepresentable step size
      Given no configured bound prevents the tested beta proposal
      And all raw arithmetic before exponentiation is finite
      And the proposed beta produces "<result>"
      When I update
      Then I receive a numerical error identifying the step-size stage
      And the committed state is unchanged

      Examples:
        | result                        |
        | exp(beta) equal to zero       |
        | exp(beta) equal to infinity   |

    Scenario: C20 Do not use clipping to conceal invalid arithmetic
      Given a valid learner using optional safety bounds
      And finite inputs whose raw meta update overflows
      When I update
      Then I receive a numerical error before bounds are applied
      And the committed state is unchanged

  @v0_1 @core @fast
  Rule: A complete snapshot continues the same learning process

    Scenario: C21 Resume without resetting adaptation
      Given a learner trained on a deterministic prefix
      When I take a full snapshot and restore a second learner from it
      Then both have identical configuration, weights, betas and traces
      When both process the same remaining examples on the same build and target
      Then each report and committed state agree bit for bit

    Scenario Outline: C22 Reject an invalid snapshot
      Given an owned snapshot with "<fault>"
      When I restore a learner from it
      Then I receive an invalid-state error
      And no learner is produced

      Examples:
        | fault                                   |
        | empty state vectors                     |
        | state vectors of different lengths      |
        | a non-finite weight                     |
        | a non-finite beta                       |
        | a non-finite trace                      |
        | invalid configuration                   |
        | beta outside its configured bounds      |
        | beta whose exponential is zero          |
        | beta whose exponential is infinite      |

    @serde
    Scenario: C25 Save and restore a learner through serde
      Given the "serde" feature is enabled and a learner trained on a deterministic prefix
      When I serialize the learner and deserialize it as a learner
      Then its configuration, weights, betas and traces agree bit for bit
      And both learners give the same reports for the same remaining examples
      When the serialized form has an invalid configuration, invalid bounds, an unknown field or a state that from_state rejects
      Then deserializing a learner returns the matching error
      And deserializing only a State returns the same error

  @autostep
  Rule: Autostep follows Table 1 of Mahmood, Sutton, Degris and Pilarski (2012)

    Scenario: A01 Match an independent reference
      Given Autostep fixtures computed in 50-digit decimal arithmetic, one with the paper's defaults and one with mu 0.5 and tau 2
      When the learner processes each fixture sequence
      Then every prediction, error, effective step size, weight, step size, trace and normalizer agrees within the equation tolerance
      And at least one step has an effective step size above one

    Scenario: A02 Construct with the paper's defaults
      Given an initial step size
      When I create an Autostep configuration
      Then mu is 0.01 and tau is 10,000
      And a non-positive initial step size, a negative or non-finite mu and a tau below 1 are rejected

    Scenario: A03 Never overshoot the current target
      Given any example
      When the learner updates on it
      Then the new error has the same sign as the old error, or is zero, and is not larger
      And if sum(alpha_i x_i^2) exceeded one, every step size was divided by it

    Scenario: A04 Fail without changing state
      Given an input, a target or intermediate value that fails at any stage
      When I update
      Then I receive the error with its stage and coordinate
      And weights, step sizes, traces and normalizers are bit-for-bit unchanged

    Scenario: A05 Resume from a snapshot
      Given a trained Autostep learner
      When I restore a second learner from its snapshot and both process the same examples
      Then reports and states agree bit for bit
      And a snapshot with an empty, mismatched, non-finite, non-positive step size or negative normalizer entry is rejected

    Scenario: A06 Learn raw targets with the defaults
      Given targets in raw milliseconds and the paper's default mu and tau
      When the learner processes 2,000 samples
      Then every update succeeds and the final error is below 1 millisecond

    Scenario: A07 Bound the step-size change per update
      Given any example and mu
      When the learner updates
      Then each step size, before the overshoot guard, changes by a factor between exp(-mu) and exp(mu)
      And every normalizer stays nonnegative

    Scenario: A08 Avoid per-example heap allocation
      Given an Autostep learner with 64 inputs
      When it predicts and updates 2,000 times, including rejected updates
      Then the counting allocator records no allocation

  @v0_1 @core @performance
  Rule: The ordinary learning path has predictable resource use

    Scenario: C23 Avoid per-example heap allocation in the core
      Given a constructed learner with O(n) candidate state allocated up front for atomic updates
      And preallocated input storage
      And allocation measurement excludes setup, test-harness work and snapshots
      When I repeatedly predict, update successfully and inspect committed state
      And I consume step-size iterators without collecting them
      Then the measured core operations perform zero heap allocations

    Scenario: C24 Report measured performance honestly
      Given benchmarks for 20, 1000 and 10000 dense features
      When prediction and update performance are measured separately
      Then the results include hardware, compiler, build flags and bounds settings
      And they include the input sizes, sample counts and measurement method
      And browser binding and rendering costs are reported separately from core timings

  @v0_1 @evaluation
  Rule: Usefulness is supported by reproducible experiments

    Scenario: E01 Evaluate tracking against a tuned LMS baseline
      Given versioned streams for stationary, drifting and changing-relevance tasks
      And a recorded LMS alpha grid selected using separate tuning seeds
      And at least 10 recorded evaluation seeds shared by every learner
      When I evaluate IDBD and the selected LMS configuration
      Then both receive identical features and targets in the same order
      And the report includes pre-update MSE, warm-up and measured intervals
      And it includes raw per-seed results, summary uncertainty and failure counts
      And failed runs remain visible in the results
      And any comparative performance claim matches the measured workload

    Scenario: E04 Reproduce Sutton Experiment 1 and Figure 3
      Given 20 independent standard-normal inputs and five signed relevant coefficients
      And exactly one of the first five coefficients is selected at random and changes sign every 20 examples
      And every learner processes 20000 warm-up and 10000 measured examples per run
      And IDBD starts with alpha 0.05 on every feature
      When I sweep fixed LMS alpha and IDBD theta using recorded parameter grids and random seeds
      Then pre-update squared error is averaged only over the 10000 measured examples
      And each parameter setting reports raw per-seed values, uncertainty and numerical failures
      And the plot compares the measured curves with Figure 3 as an approximate replication
      And any difference from the paper's unspecified random generator, seed or grid is stated
      And numerical results are never claimed to exactly reproduce unpublished samples

    Scenario: E02 Distinguish Sutton Experiment 2 from the adapted demonstration
      Given the Sutton Experiment 2 tracking task with 20 standard-normal features
      And the same five coefficients remain relevant while one changes sign every 20 samples
      When I run the 250000-sample paper configuration
      Then initial alpha is 0.05, theta is 0.001 and optional bounds are disabled
      And the seed, random generator and sign-change timing are recorded
      And relevant and irrelevant alpha trajectories are compared with Figure 4
      And an optional fixed-rate sweep for relevant inputs is compared with Figure 5
      And a run with theta 0.01 is labeled as an adapted demonstration

    Scenario: E05 Evaluate Autostep against tuned baselines
      Given the E01 tasks, seeds and protocol
      When Autostep runs with mu tuned on the tuning seeds and, separately, at the paper's default mu without tuning
      Then its evaluation MSE is reported beside tuned LMS and tuned IDBD, with paired differences by seed
      And the MSE at every mu on the grid is reported for every task
      And every failed run is reported with its sample and stage, never replaced by a value
      And the report claims no general ordering of the learners

    Scenario: E06 Measure the thermostat house behind the story page
      Given a 6-sensor house where 3 sensors switch between a strong and a weak influence
      And the same house with switching turned off
      When LMS and IDBD are tuned with the E01 protocol and Autostep runs at its defaults
      Then every learner's evaluation MSE is reported for both houses, with paired differences by seed
      And IDBD's final step size is reported for every sensor
      And the report states that the house is synthetic and claims no general ordering

    Scenario: E03 Integrate the crate in a practical streaming example
      Given a documented example of latency prediction under a deployment change
      And its data is explicitly identified as synthetic or measured
      When I run it from the documented Cargo command
      Then predictions are recorded before learning each target
      And the example reports units, input preparation and error over time
      And a complete checkpoint can resume the learner
      And it demonstrates how callers handle an invalid sample
      And any evaluation of the resumed application also restores its stream position

  @demo @integration
  Rule: The visual demo runs the actual Rust implementation

    Scenario: D01 Use the core learner through WebAssembly
      Given the demo has loaded its WASM module
      When I run the C06 and C07 reference samples through the browser adapter
      Then the exposed predictions, weights, betas and traces match the fixtures within tolerance
      And the build uses the same versioned Rust core as the crate
      And the adapter delegates learning updates to that core

    Scenario: D02 Keep every visual on the same completed sample
      Given a running simulated task
      When sample t has completed for both learners
      Then the feature view and model-state view show state after sample t
      And the error chart records predictions made before learning sample t
      And every panel identifies the same completed sample index
      And the table and chart tooltips agree with the displayed state

  @demo @interaction
  Rule: Visitors can observe and control adaptation

    Scenario: D14 Explain one IDBD update before playback
      Given the initial demo presents a small selected feature and a sample
      When I advance its guided step
      Then I see the pre-update prediction and residual
      And the previous trace and residual explain the direction of the beta adjustment
      And the new alpha is shown before the weight and trace updates
      And the old and new values are labeled with the same sample index
      And the values come from the Rust core rather than a separate UI implementation
      And I can move from the guided step into the full experiment

    Scenario: D03 Run, pause and single-step the experiment
      Given a loaded demo paused at completed sample t
      When I choose "Step"
      Then exactly one shared sample is processed
      And both learners and every panel advance to t + 1
      When I choose "Run" and later choose "Pause"
      Then processing stops at a completed sample boundary
      And the state remains fixed until the next user action

    Scenario: D12 Distinguish the paper task from the extension
      Given the selectable Sutton 1992 preset and changing-relevance extension
      When I switch between their descriptions
      Then the Sutton preset says the same five inputs stay relevant while one coefficient flips sign every 20 samples
      And the changing-relevance preset says it is a new stress test, not a paper reproduction
      And each view identifies its preset, seed, algorithm version and settings
      And the comparison includes the initial transient and does not assert universal superiority

    Scenario: D04 Change the world without resetting learned state
      Given the demo is paused after sample t
      When I choose "Change the world"
      Then a deterministic relevance-swap event is scheduled before sample t + 1
      And the learner state remains unchanged until a sample is processed
      When I choose "Step"
      Then the target-generating relationship changes before sample t + 1
      And the same changed sample is given to IDBD and LMS
      And a labeled event marker appears at t + 1
      And the event itself does not reset either learner
      And the sample updates start from each learner's state after t
      And subsequent visuals show the actual recovery, including slow recovery or regressions

    Scenario: D05 Reveal what an adaptive step size means
      Given a paused experiment with multiple features
      When I select a feature using pointer or keyboard
      Then I can inspect its current input, true coefficient, learned weight and alpha
      And the alpha history uses a labeled logarithmic scale
      And the explanation describes alpha as the size of a learning adjustment
      And the true coefficient is labeled as known simulator information
      And alpha is not labeled as feature importance or confidence

    Scenario: D13 Export a reproducible run
      Given a completed or paused experiment
      When I export its run definition and results
      Then the export includes seed, generator version, preset, algorithm and build version, initial settings and ordered events
      And it includes the sample intervals, raw metrics and any failure
      And documented commands reproduce the run with the same build
      And the shared view describes synthetic data as synthetic

    Scenario: D06 Reproduce an experiment independently of animation speed
      Given a seed, initial configuration and scheduled event log
      When I reset each run and reload the recorded event schedule
      And I replay the first 1000 samples at two playback speeds
      Then the same build produces identical final learner states
      And every metric uses the same sample-indexed observations
      And reset clears charts, the live pending-event queue and metric windows
      And changing an algorithm hyperparameter starts a new labeled run

    Scenario: D07 Compare against LMS on a shared stream
      Given the IDBD and fixed-step LMS comparison is enabled
      When the experiment advances
      Then both learners receive the same features and target for every sample
      And the UI displays each learner's configuration
      And a "tuned LMS" label appears only with a recorded separate tuning procedure
      And each rolling MSE uses the last min(100, completed samples) pre-update errors
      And changing the chart view does not change the learning process

    Scenario: D08 Handle a numerical failure visibly
      Given a sample causes one learner to return a numerical error
      When that sample is attempted
      Then the experiment pauses
      And the UI identifies the learner, attempted sample and numerical failure
      And the failed learner retains its last valid committed state
      And all published panels, tables, tooltips and comparative metrics remain at the last sample completed by both learners
      And the failure stays visible in the run summary
      And no non-finite result is plotted or replaced with zero
      And playback requires reset before it can resume
      And reset starts a fresh valid experiment

  @demo @presentation
  Rule: A polished interface remains readable and accessible

    Scenario: D09 Read and operate the demo on a narrow screen
      Given a viewport width of 390 pixels
      When I inspect the default experiment and open feature details
      Then controls, axes and labels fit without page-level horizontal scrolling
      And the main layout stacks while preserving the experiment controls
      And the same functions are available at a viewport width of 1440 pixels
      And data marks are not used as the only way to read exact values

    Scenario: D10 Operate without relying on color or animation
      Given a visitor uses keyboard navigation and reduced-motion preferences
      When the visitor runs, pauses, steps and changes the world
      Then every control has an accessible name and visible focus
      And optional transitions respect reduced-motion preferences
      And labels or shapes distinguish IDBD, LMS and true coefficients
      And a text/table view exposes the selected feature and current metrics

    Scenario: D16 Show untuned Autostep on the same stream
      Given the live experiment
      When it advances
      Then Autostep learns from the same features and target as IDBD and LMS
      And its configuration is labeled as the paper's defaults, not tuned to the task
      And its rolling MSE is drawn with a distinct color and dash pattern, and listed with its weights and step sizes
      And if any of the three learners rejects a sample, none of them learns from it

    Scenario: S01 Explain IDBD to a non-specialist with the thermostat house
      Given the story page, built on the house measured in scenario E06
      When it loads
      Then every step size, prediction and error it shows comes from the crate through WebAssembly
      And chapter curves that are sketches say so
      And it states the E06 result for both the switching house and the still house

    Scenario: S02 Show one IDBD update in slow motion
      Given a fresh learner on the house
      When the visitor asks for the next reading
      Then the page shows the guess, the real change and the miss read from the crate
      And for every sensor it shows the trust and the speed before and after the update

    Scenario: S03 Let a visitor race the learners by hand
      Given the game on the story page, with a still house and a switching house
      When the visitor sets trust sliders and starts the race
      Then IDBD and the fixed-step learner use the parameters E06 selected for that house
      And all three scores average the same 300 most recent readings
      And each slider shows what its setting costs per reading
      And Reset restores the readings, the sliders and the hidden step-size panel

    Scenario: S06 Keep the story page usable without WebGL or motion
      Given a browser without WebGL, or a visitor who prefers reduced motion
      When the story page loads
      Then the house is drawn in 2D from the same step sizes

    Scenario: D15 Pass automated WCAG checks in every supported browser
      Given the page at rest, after a run with a world change, and in the dark color scheme
      When axe-core checks the WCAG 2.2 A and AA rules in Chromium, Firefox and WebKit
      Then it reports no violations

    Scenario: D11 Keep playback bounded and responsive
      Given the default experiment has 20 features and a rolling chart history
      When playback runs for 100000 samples
      Then only the most recent 2000 samples are retained for chart display
      And aggregate metrics remain available for all completed samples
      And rendering frequency does not alter sample generation or learning
      And simulation batches yield so that pause and keyboard controls remain usable

  @release @review
  Rule: Release claims match the implemented and verified scope

    Scenario: R01 Release the core with a traceable acceptance record
      Given the v0.1 core and evaluation scenarios have implementation mappings
      When a release candidate is reviewed
      Then all mandatory core scenarios pass
      And an approximate Experiment 1 replication and measured core performance are attached
      And longer experiments and the practical streaming example are recorded as follow-on work unless completed
      And the documented minimum Rust version is tested
      And the default core dependency graph has no runtime dependencies
      And the Cargo package includes its licenses, paper citation, equations and runnable docs.rs example
      And the limitations describe feature scaling, correlated inputs and nonstationary streams
      And the reference fixtures identify independent calculations and provenance
      And cargo publish --dry-run passes
      And the crate name is checked before publication
      And the release notes state numerical and evaluation limitations

    Scenario: R02 Ship the demo as a separate deliverable
      Given the core release candidate passes its required scenarios
      And D01, D02, D03, D07, D12 and D14 have been verified as demo release gates
      And remaining interaction and presentation scenarios are tracked as follow-on work
      When the demo release is prepared
      Then its Rust core version and experiment settings are inspectable
      And its visual comparison labels match the evaluation evidence
      And a public project overview links paper reproduction, extension, source code and reproducible run instructions
      And the demo's WASM dependencies do not enter the core's default dependency graph
      And the demo is delivered as a static browser app
      And deployment verification and any remaining browser-test limits are recorded
