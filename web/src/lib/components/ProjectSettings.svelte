<script lang="ts">
  let {
    projectType = 'ec2',
    awsRegion = 'eu-west-1',
    azureRegion = 'swedencentral'
  }: {
    projectType?: 'ec2' | 'ec2_vm' | 'rds' | 'on_prem';
    awsRegion?: string;
    azureRegion?: string;
  } = $props();
</script>

<section class="settings-band" aria-labelledby="settings-heading">
  <h2 id="settings-heading">Project settings</h2>
  <dl>
    <div>
      <dt>Project type</dt>
      <dd>{projectType === 'on_prem' ? 'On-prem' : projectType.replace('_', ' ').toUpperCase()}</dd>
    </div>
    <div>
      <dt>Source region</dt>
      <dd>{projectType === 'on_prem' ? 'Not applicable' : awsRegion}</dd>
    </div>
    <div>
      <dt>Azure region</dt>
      <dd>{azureRegion}</dd>
    </div>
    <div>
      <dt>Currency</dt>
      <dd>USD</dd>
    </div>
  </dl>
</section>

<style>
  .settings-band {
    padding: 14px 16px;
    color: var(--ink);
    background: var(--surface);
    border-block: 1px solid var(--border);
  }
  h2 {
    margin: 0 0 10px;
    font-size: 1rem;
  }
  dl {
    display: grid;
    grid-template-columns: repeat(4, minmax(130px, 1fr));
    gap: 14px;
    margin: 0;
  }
  dt {
    color: var(--muted);
    font-size: 0.75rem;
  }
  dd {
    margin: 3px 0 0;
    font-weight: 650;
  }
  @media (max-width: 680px) {
    dl {
      grid-template-columns: repeat(2, minmax(120px, 1fr));
    }
  }
</style>
