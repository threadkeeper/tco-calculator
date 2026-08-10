<script lang="ts">
  let { resourceCount = 0 }: { resourceCount?: number } = $props();
</script>

<section class="resource-table" aria-labelledby="resources-heading">
  <div class="resource-heading">
    <h2 id="resources-heading">Resources</h2>
    <span>{resourceCount} {resourceCount === 1 ? 'resource' : 'resources'}</span>
  </div>
  <div class="empty"><strong>No resources</strong><span>Add a source SQL Server workload to calculate an estimate.</span></div>
</section>

<style>
  .resource-table { min-height: 260px; background: white; border-block: 1px solid #cad4d7; }
  .resource-heading { display: flex; align-items: center; justify-content: space-between; padding: 11px 14px; background: #f7f9f9; border-bottom: 1px solid #cad4d7; }
  h2 { margin: 0; font-size: 1rem; }
  .resource-heading span { color: #5c6d72; font-size: 0.8rem; }
  .empty { min-height: 205px; display: grid; place-content: center; justify-items: center; gap: 4px; color: #5c6d72; text-align: center; }
  .empty strong { color: #182428; }
</style>