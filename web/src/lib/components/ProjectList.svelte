<script lang="ts">
  import { Database, Plus } from 'lucide-svelte';
  import { text } from '$lib/i18n/en';

  let { onnew }: { onnew: () => void } = $props();
</script>

<section class="title-band" aria-labelledby="projects-heading">
  <div>
    <p class="eyebrow">Current estimates</p>
    <h1 id="projects-heading">{text.projects}</h1>
  </div>
  <button class="button primary" type="button" onclick={onnew}>
    <Plus size={18} aria-hidden="true" />
    <span>{text.newProject}</span>
  </button>
</section>

<aside class="guest-notice">
  <strong>{text.guestDraft}</strong>
  <span>{text.guestNotice}</span>
</aside>

<section class="project-table" aria-label={text.projects}>
  <div class="table-header" aria-hidden="true">
    <span>Name</span>
    <span>Type</span>
    <span>Modified</span>
    <span>Source region</span>
    <span>Azure region</span>
    <span>Resources</span>
    <span>Source annual</span>
    <span>Azure annual</span>
    <span>Actions</span>
  </div>
  <div class="empty-state">
    <Database size={28} aria-hidden="true" />
    <p>{text.noProject}</p>
    <span>{text.noProjectDetail}</span>
  </div>
</section>