Feature: Label a node, this will be used while scheduling replica of volume considering the
        node topology

  Background:
    Given a control plane, two Io-Engine instances, two pools

  Scenario: Label a node
    Given an unlabeled node
    When the user issues a label command with a label to the node
    Then the given node should be labeled with the given label

  Scenario: UnLabel a node
    Given an labeled node
    When the user issues a unlabel command with a label key to the node
    Then the given node should remove the label with the given key

  Scenario: Overwite the label of node
    Given an labeled node
    When the user issues a label command with a same key and different value to the node
    Then the given node should overwrite the label with the given key
