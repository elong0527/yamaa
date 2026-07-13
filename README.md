# yamaa

`yamaa` is an open-source solution designed to have a language-agnostic YAML schema to map and derive CDISC data from ODM to SDTM and ADaM.

## Project Structure

The project is organized into several key layers to separate concerns and logic:

- **`orchestrator/`**: Coordinates the overall project workflows and integrations using AI Agents.
- **`yaml/`**: Stores the YAML schema definition and mappings used across the project. This is the core part of the project.
- **`R/`**: Contains the R packages used to realize the workflow with R.
- **`python/`**: Contains the Python packages used to realize the workflow with Python.


Each directory contains an `agents.md` file with specific development guidelines for that layer.
