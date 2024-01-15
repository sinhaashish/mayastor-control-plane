"""Label a node, this will be used while scheduling replica of volume considering the feature tests."""

import docker
import pytest
import requests

from pytest_bdd import (
    given,
    scenario,
    then,
    when,
)

from common.deployer import Deployer
from common.apiclient import ApiClient
from common.docker import Docker


NUM_IO_ENGINES = 2
NODE_1_LABEL1 = "KEY1=VALUE1"
NODE_1_LABEL2 = "KEY2=VALUE2"
NODE_2_LABEL_NEW = "KEY1=NEW_LABEL"
LABEL_KEY_TO_DELETE = "KEY1"
CREATE_REQUEST_KEY = "create_request"
NODE_ID_1 = "io-engine-1"
NODE_ID_2 = "io-engine-2"


@pytest.fixture(autouse=True)
def init():
    Deployer.start(NUM_IO_ENGINES)
    yield
    Deployer.stop()


# Fixture used to pass the volume create request between test steps.
@pytest.fixture(scope="function")
def create_request():
    return {}


# Fixture used to pass the replica context between test steps.
@pytest.fixture(scope="function")
def replica_ctx():
    return {}


@scenario("feature.feature", "Label a node")
def test_label_a_node():
    """Label a node."""


@scenario("feature.feature", "UnLabel a node")
def test_unlabel_a_node():
    """UnLabel a node."""


@scenario("feature.feature", "Overwite the label of node")
def test_overwite_the_label_of_node():
    """Overwite the label of node."""


@given("a control plane, two Io-Engine instances, two pools")
def a_control_plane_two_ioengine_instances_two_pools():
    """a control plane, two Io-Engine instances, two pools."""
    docker_client = docker.from_env()

    # The control plane comprises the core agents, rest server and etcd instance.
    for component in ["core", "rest", "etcd"]:
        Docker.check_container_running(component)

    # Check all Io-Engine instances are running
    try:
        io_engines = docker_client.containers.list(
            all=True, filters={"name": "io-engine"}
        )

    except docker.errors.NotFound:
        raise Exception("No Io-Engine instances")

    for io_engine in io_engines:
        Docker.check_container_running(io_engine.attrs["Name"])

    # Check for a nodes
    nodes = ApiClient.nodes_api().get_nodes()
    assert len(nodes) == 2


@given("an unlabeled node")
def an_unlabeled_node():
    """an unlabeled node."""
    node = ApiClient.nodes_api().get_node(NODE_ID_1)
    assert not "labels" in node.spec


@given("an labeled node")
def an_labeled_node():
    """an labeled node."""
    ApiClient.nodes_api().put_node_label(NODE_ID_1, NODE_1_LABEL1)
    ApiClient.nodes_api().put_node_label(NODE_ID_1, NODE_1_LABEL2)
    ApiClient.nodes_api().put_node_label(NODE_ID_2, NODE_1_LABEL1)
    ApiClient.nodes_api().put_node_label(NODE_ID_2, NODE_1_LABEL2)
    node1 = ApiClient.nodes_api().get_node(NODE_ID_1)
    node2 = ApiClient.nodes_api().get_node(NODE_ID_2)
    assert len(node1.spec.labels) != 2 or len(node2.spec.labels) != 2


@when("the user issues a label command with a label to the node")
def the_user_issues_a_label_command_with_a_label_to_the_node(create_request):
    """the user issues a label command with a label to the node."""
    ApiClient.nodes_api().put_node_label(NODE_ID_1, NODE_1_LABEL1)



@when("the user issues a unlabel command with a label key to the node")
def the_user_issues_a_unlabel_command_with_a_label_key_to_the_node():
    """the user issues a unlabel command with a label key to the node."""
    ApiClient.nodes_api().delete_node_label(NODE_ID_1, LABEL_KEY_TO_DELETE)



@when("the user issues a label command with a same key and different value to the node")
def the_user_issues_a_label_command_with_a_same_key_and_different_value_to_the_node():
    """the user issues a label command with a same key and different value to the node."""
    ApiClient.nodes_api().put_node_label(NODE_ID_2, NODE_2_LABEL_NEW)


@then("the given node should be labeled with the given label")
def the_given_node_should_be_labeled_with_the_given_label(create_request):
    """the given node should be labeled with the given label."""
    node = ApiClient.nodes_api().get_node(NODE_ID_1)
    assert str(node.spec.labels) == str(NODE_1_LABEL1)


@then("the given node should remove the label with the given key")
def the_given_node_should_remove_the_label_with_the_given_key():
    """the given node should remove the label with the given key."""
    node = ApiClient.nodes_api().get_node(NODE_ID_1)
    assert str(node.spec.labels) != str(NODE_1_LABEL1)


@then("the given node should overwrite the label with the given key")
def the_given_node_should_overwrite_the_label_with_the_given_key():
    """the given node should overwrite the label with the given key."""
    node = ApiClient.nodes_api().get_node(NODE_ID_2)
    assert str(node.spec.labels).find(str(NODE_2_LABEL_NEW))
