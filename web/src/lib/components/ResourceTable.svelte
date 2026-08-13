<script lang="ts">
  let { resourceCount = 0 }: { resourceCount?: number } = $props();
</script>

<section class="resource-table" aria-labelledby="resources-heading">
  <div class="resource-heading">
    <h2 id="resources-heading">Resources</h2>
    <span>{resourceCount} {resourceCount === 1 ? 'resource' : 'resources'}</span>
  </div>
  <div class="empty">
    <strong>No resources</strong><span
      >Add a source SQL Server workload to calculate an estimate.</span
    >
  </div>
</section>

<style>
  .resource-table {
    min-height: 260px;
    background: var(--surface);
    border-block: 1px solid var(--border);
  }
  .resource-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 11px 14px;
    background: var(--surface-subtle);
    border-bottom: 1px solid var(--border);
  }
  h2 {
    margin: 0;
    font-size: 1rem;
  }
  .resource-heading span {
    color: var(--muted);
    font-size: 0.8rem;
  }
  .empty {
    min-height: 205px;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 4px;
    color: var(--muted);
    text-align: center;
  }
  .empty strong {
    color: var(--ink);
  }
</style>
