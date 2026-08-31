require "minitest/autorun"
require "tempfile"

require_relative "check_example_dependencies"

class TestExampleDependencies < Minitest::Test
  def test_between_value_is_a_lookup_dependency
    spec = Tempfile.new(["between-dependency", ".yaml"])
    spec.write(<<~YAML)
      record_lookups:
        - id: WINDOW
          dataset: WINDOWS
          source: STUDYID
          key: STUDYID
          between: {value: ADY, lower: AWLO, upper: AWHI}
      output:
        columns: [STUDYID, AVISIT, ADY]
      columns:
        - {name: STUDYID, derivation: {literal: STUDY}}
        - {name: AVISIT, derivation: {source: WINDOW.AVISIT}}
        - {name: ADY, derivation: {literal: 1}}
    YAML
    spec.close

    assert_includes check(spec.path), "AVISIT references later column ADY"
  ensure
    spec&.unlink
  end
end
