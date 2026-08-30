# yamaa <img src="https://github.com/elong0527/yamaa/raw/main/docs/assets/logo.png" align="right" width="120" />

`yamaa` is a language-agnostic YAML schema that define the specification to derive clinical trial data from ODM to SDTM and ADaM following CDISC standard.

## Project Structure

The project is organized into several key layers to separate concerns and logic:

- **`orchestrator/`**: Coordinates the overall project workflows and integrations using AI Agents.
- **`docs`**: Documentations
- **`yaml/`**: Stores the YAML schema definition and mappings used across the project. This is the core part of the project.
- **`R/`**: Contains the R packages used to realize the workflow with R.
- **`python/`**: Contains the Python packages used to realize the workflow with Python.


Each directory contains an `agents.md` file with specific development guidelines for that layer.
