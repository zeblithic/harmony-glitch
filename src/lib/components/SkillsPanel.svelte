<script lang="ts">
  import type { SkillDef, SkillProgressFrame } from '../types';
  import { learnSkill, cancelLearning } from '../ipc';

  let {
    skills = [],
    skillProgress = null,
    imagination = 0,
    visible = false,
    onClose,
  }: {
    skills: SkillDef[];
    skillProgress: SkillProgressFrame | null;
    imagination: number;
    visible: boolean;
    onClose?: () => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;
  let selectedSkillId = $state<string | null>(null);
  let actionError = $state<string | null>(null);

  $effect(() => {
    if (visible && dialogEl) {
      previousFocus = document.activeElement as HTMLElement | null;
      if (!dialogEl.open) dialogEl.showModal();
      const first = dialogEl.querySelector<HTMLElement>('button[role="option"]');
      first?.focus();
    } else if (!visible && dialogEl?.open) {
      dialogEl.close();
      previousFocus?.focus();
      previousFocus = null;
    }
  });

  function handleCancel(e: Event) {
    e.preventDefault();
    onClose?.();
  }

  function skillStatus(skill: SkillDef): 'learned' | 'learning' | 'available' | 'locked' {
    if (skillProgress?.learned.includes(skill.id)) return 'learned';
    if (skillProgress?.learning?.skillId === skill.id) return 'learning';
    const prereqsMet = skill.prerequisites.every(p => skillProgress?.learned.includes(p));
    if (prereqsMet) return 'available';
    return 'locked';
  }

  function canAfford(skill: SkillDef): boolean {
    return imagination >= skill.imaginationCost;
  }

  function missingPrereq(skill: SkillDef): string | null {
    for (const p of skill.prerequisites) {
      if (!skillProgress?.learned.includes(p)) {
        const def = skills.find(s => s.id === p);
        return def?.name ?? p;
      }
    }
    return null;
  }

  let sortedSkills = $derived.by(() => {
    const order = ['learned', 'learning', 'available', 'locked'] as const;
    return [...skills].sort((a, b) => {
      const ai = order.indexOf(skillStatus(a));
      const bi = order.indexOf(skillStatus(b));
      if (ai !== bi) return ai - bi;
      return a.name.localeCompare(b.name);
    });
  });

  let selectedSkill = $derived.by(() => {
    if (!selectedSkillId) return null;
    return skills.find(s => s.id === selectedSkillId) ?? null;
  });

  function statusIcon(status: string): string {
    switch (status) {
      case 'learned': return '\u2713';
      case 'learning': return '\u23F3';
      case 'available': return '\u25CB';
      case 'locked': return '\uD83D\uDD12';
      default: return '';
    }
  }

  function formatTime(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = Math.ceil(secs % 60);
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  }

  async function handleLearn() {
    if (!selectedSkillId) return;
    actionError = null;
    try {
      await learnSkill(selectedSkillId);
    } catch (e) {
      actionError = String(e);
    }
  }

  async function handleCancel2() {
    actionError = null;
    try {
      await cancelLearning();
    } catch (e) {
      actionError = String(e);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose?.();
      return;
    }
    // Arrow navigation between skill rows. Handled on <dialog> rather than
    // the listbox container: the listbox div isn't a real interactive
    // element in the DOM, so putting keydown on it trips the svelte a11y
    // linter — and the dialog is already handling Escape, so this keeps
    // all keyboard input on one host.
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const options = Array.from(
        dialogEl?.querySelectorAll<HTMLElement>('button[role="option"]') ?? []
      );
      const idx = options.findIndex(el => el === document.activeElement);
      if (idx === -1) return;
      if (e.key === 'ArrowDown' && idx < options.length - 1) {
        e.preventDefault();
        options[idx + 1].focus();
      } else if (e.key === 'ArrowUp' && idx > 0) {
        e.preventDefault();
        options[idx - 1].focus();
      }
    }
  }
</script>

{#if visible}
  <dialog
    class="skills-panel"
    aria-label="Skills"
    aria-modal="true"
    bind:this={dialogEl}
    oncancel={handleCancel}
    onkeydown={handleKeyDown}
  >
    <div class="panel-header">
      <h2>Skills</h2>
      <button type="button" class="close-btn" onclick={() => onClose?.()} aria-label="Close">
        &times;
      </button>
    </div>

    <div class="skill-list" role="listbox" aria-label="Skills">
      {#each sortedSkills as skill (skill.id)}
        {@const status = skillStatus(skill)}
        <button
          type="button"
          role="option"
          aria-selected={selectedSkillId === skill.id}
          class="skill-row"
          class:learned={status === 'learned'}
          class:learning={status === 'learning'}
          class:available={status === 'available'}
          class:locked-skill={status === 'locked'}
          class:selected={selectedSkillId === skill.id}
          aria-label="{skill.name} ({status})"
          onclick={() => { selectedSkillId = selectedSkillId === skill.id ? null : skill.id; actionError = null; }}
        >
          <span class="skill-status">{statusIcon(status)}</span>
          <span class="skill-name">{skill.name}</span>
        </button>
      {/each}
    </div>

    {#if selectedSkill}
      {@const status = skillStatus(selectedSkill)}
      <div class="skill-details">
        <div class="skill-detail-name">{selectedSkill.name}</div>
        <div class="skill-desc">{selectedSkill.description}</div>

        {#if selectedSkill.prerequisites.length > 0}
          <div class="detail-section">
            <div class="detail-label">Requires:</div>
            {#each selectedSkill.prerequisites as prereqId}
              {@const prereqDef = skills.find(s => s.id === prereqId)}
              {@const met = skillProgress?.learned.includes(prereqId)}
              <div class="detail-item" class:sufficient={met}>
                {prereqDef?.name ?? prereqId} {met ? '\u2713' : '\u2717'}
              </div>
            {/each}
          </div>
        {/if}

        <div class="detail-section">
          <div class="detail-label">Cost:</div>
          <div class="detail-item" class:sufficient={canAfford(selectedSkill)}>
            {selectedSkill.imaginationCost} Imagination ({imagination} available)
          </div>
        </div>

        <div class="detail-section">
          <div class="detail-label">Learn time:</div>
          <div class="detail-item">{formatTime(selectedSkill.learnTimeSecs)}</div>
        </div>

        {#if selectedSkill.unlocksRecipes.length > 0}
          <div class="detail-section">
            <div class="detail-label">Unlocks:</div>
            {#each selectedSkill.unlocksRecipes as recipeId}
              <div class="detail-item">{recipeId.replace(/_/g, ' ')}</div>
            {/each}
          </div>
        {/if}

        {#if status === 'learning' && skillProgress?.learning}
          <div class="learning-progress">
            <div
              class="progress-bar"
              role="progressbar"
              aria-label="Learning in progress"
              aria-valuenow={Math.round((skillProgress.learning.progress) * 100)}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuetext="{formatTime(skillProgress.learning.remainingSecs)} remaining"
            >
              <div class="progress-fill" style="width: {skillProgress.learning.progress * 100}%"></div>
            </div>
            <span class="progress-label">
              {formatTime(skillProgress.learning.remainingSecs)}
            </span>
          </div>
          <button type="button" class="cancel-btn" onclick={handleCancel2}>
            Cancel Learning
          </button>
        {:else if status === 'learned'}
          <div class="learned-badge" role="status">Learned</div>
        {:else if status === 'available'}
          <button
            type="button"
            class="learn-btn"
            disabled={!canAfford(selectedSkill) || skillProgress?.learning != null}
            onclick={handleLearn}
          >
            {skillProgress?.learning != null ? 'Already learning...' : 'Learn'}
          </button>
        {:else if status === 'locked'}
          <div class="locked-notice" role="status">
            Requires: {missingPrereq(selectedSkill)}
          </div>
        {/if}

        {#if actionError}
          <div class="action-error" role="alert">{actionError}</div>
        {/if}
      </div>
    {/if}
  </dialog>
{/if}

<style>
  .skills-panel {
    position: fixed;
    top: 0;
    right: 0;
    left: auto;
    width: 240px;
    height: 100%;
    max-height: 100%;
    margin: 0;
    padding: 0;
    border: none;
    border-left: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(26, 26, 46, 0.95);
    color: #e0e0e0;
    font-size: 12px;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    z-index: 100;
  }

  .skills-panel::backdrop {
    background: transparent;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }

  .panel-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: bold;
    color: #c084fc;
  }

  .close-btn {
    background: none;
    border: none;
    color: #999;
    font-size: 18px;
    cursor: pointer;
    padding: 0 4px;
  }

  .close-btn:hover {
    color: #fff;
  }

  .skill-list {
    padding: 4px;
    overflow-y: auto;
    flex-shrink: 0;
    max-height: 200px;
  }

  .skill-row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 8px;
    margin-bottom: 2px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid transparent;
    border-radius: 4px;
    cursor: pointer;
    color: #ccc;
    font-size: 12px;
    text-align: left;
  }

  .skill-row:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .skill-row.selected {
    border-color: #c084fc;
    background: rgba(192, 132, 252, 0.1);
  }

  .skill-row.learned {
    color: #86efac;
  }

  .skill-row.learning {
    color: #fbbf24;
  }

  .skill-row.available {
    color: #e0e0e0;
  }

  .skill-row.locked-skill {
    color: #666;
  }

  .skill-status {
    width: 16px;
    text-align: center;
    flex-shrink: 0;
  }

  .skill-name {
    flex: 1;
  }

  .skill-details {
    padding: 10px 12px;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    flex: 1;
    overflow-y: auto;
  }

  .skill-detail-name {
    font-size: 13px;
    font-weight: bold;
    color: #fff;
    margin-bottom: 4px;
  }

  .skill-desc {
    font-size: 11px;
    color: #aaa;
    margin-bottom: 8px;
  }

  .detail-section {
    margin-bottom: 6px;
  }

  .detail-label {
    font-size: 10px;
    color: #999;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 2px;
  }

  .detail-item {
    font-size: 11px;
    color: #f87171;
    padding: 1px 0;
    text-transform: capitalize;
  }

  .detail-item.sufficient {
    color: #86efac;
  }

  .learning-progress {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }

  .progress-bar {
    flex: 1;
    height: 8px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #a855f7, #c084fc);
    border-radius: 4px;
    transition: width 0.5s ease;
  }

  .progress-label {
    font-size: 11px;
    color: #c084fc;
    min-width: 40px;
    text-align: right;
  }

  .learn-btn {
    width: 100%;
    padding: 8px;
    margin-top: 8px;
    background: #7c3aed;
    border: none;
    border-radius: 4px;
    color: #fff;
    font-size: 12px;
    font-weight: bold;
    cursor: pointer;
  }

  .learn-btn:hover:not(:disabled) {
    background: #6d28d9;
  }

  .learn-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .cancel-btn {
    width: 100%;
    padding: 6px;
    margin-top: 4px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    color: #aaa;
    font-size: 11px;
    cursor: pointer;
  }

  .cancel-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .learned-badge {
    margin-top: 8px;
    padding: 6px;
    text-align: center;
    color: #86efac;
    font-weight: bold;
    background: rgba(134, 239, 172, 0.1);
    border-radius: 4px;
  }

  .locked-notice {
    margin-top: 8px;
    padding: 6px;
    text-align: center;
    color: #f59e0b;
    font-size: 11px;
    background: rgba(245, 158, 11, 0.1);
    border-radius: 4px;
  }

  .action-error {
    margin-top: 6px;
    padding: 4px 6px;
    color: #f87171;
    font-size: 11px;
    background: rgba(248, 113, 113, 0.1);
    border-radius: 4px;
  }
</style>
