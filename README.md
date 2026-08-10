# Azure SQL TCO

**A simple way to compare the estimated yearly cost of running SQL Server today with running it on Azure SQL Managed Instance.**

TCO means **total cost of ownership**: the wider cost of running something, not just its purchase price.

> **Project status:** The application specification is complete and ready to scaffold with GitHub Copilot agents. The working web application has not been built yet.

## What will it do?

Azure SQL TCO will help people build a clear, side-by-side cost estimate without having to understand a large spreadsheet.

It will support three kinds of projects:

- **Amazon EC2:** SQL Server running on an AWS virtual machine.
- **Amazon RDS:** SQL Server running on Amazon's managed database service.
- **On-premises:** SQL Server running on hardware owned by the organization.

Each project covers one current environment and one chosen Azure location.

## How will it work?

1. **Create a project** and choose EC2, RDS, or On-premises.
2. **Enter the current setup,** such as computing power, memory, storage needs, licensing, discounts, and operating hours.
3. **Run the estimate.** The application gathers public AWS and Azure prices where needed.
4. **Review the comparison.** It shows the current estimated yearly cost, the Azure estimate, the difference, and the suggested Azure SQL Managed Instance size.

Every result will include a plain explanation of why an Azure option was selected. If no suitable option exists, the application will say `NO MAPPING` instead of showing misleading savings.

## What will the result show?

- Estimated yearly cost of the current environment.
- Estimated yearly Azure SQL Managed Instance cost.
- Estimated saving or additional cost.
- The suggested Azure setup and size.
- The age and source of the prices used.
- Warnings for stale prices, licensing assumptions, or workloads that do not fit.

## Is it a quote?

No. Results are **planning estimates**, not invoices, contractual quotes, licensing advice, tax advice, or guarantees of savings.

Actual prices and licensing rights depend on the organization's agreements and circumstances. Any special licensing benefits must be checked with an appropriate licensing specialist. The application estimates cost and size only; it does not prove that a database is ready to move.

## Privacy and access

- Guests will be able to calculate without signing in. Their draft stays in their own browser and is not saved to the service.
- Signed-in users will be able to save private projects that only they can access.
- Project information is treated as confidential business data.
- Workload names and server identifiers must not be written to normal application logs.

## Where are we now?

The product behavior, calculations, security approach, user experience, and Azure development environment have been designed and agreed.

The next step is to scaffold the application with GitHub Copilot agents, then build and test the calculation engine, web interface, live pricing connections, private project storage, and Azure deployment.

For the detailed blueprint, see [Azure Specification.md](Azure%20Specification.md). For the recorded product decisions, see [design clarificaitons.md](design%20clarificaitons.md).
