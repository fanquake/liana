from bip32 import BIP32
from bip32.utils import _pubkey_to_fingerprint
from bip380.descriptors import Descriptor
from concurrent import futures
from test_framework.bitcoind import Bitcoind
from test_framework.lianad import Lianad
from test_framework.signer import SingleSigner, MultiSigner
from test_framework.utils import (
    EXECUTOR_WORKERS,
)

import os
import pytest
import shutil
import tempfile
import time


# A dict in which we count how often a particular test has run so far. Used to
# give each attempt its own numbered directory, and avoid clashes.
ATTEMPTS = {}


@pytest.fixture(scope="session")
def test_base_dir():
    d = os.getenv("TEST_DIR", "/tmp")

    directory = tempfile.mkdtemp(prefix="lianad-tests-", dir=d)
    print("Running tests in {}".format(directory))

    yield directory

    content = os.listdir(directory)
    if content == []:
        shutil.rmtree(directory)
    else:
        print(f"Leaving base dir '{directory}' as it still contains {content}")


# Taken from https://docs.pytest.org/en/latest/example/simple.html#making-test-result-information-available-in-fixtures
@pytest.hookimpl(tryfirst=True, hookwrapper=True)
def pytest_runtest_makereport(item, call):
    # execute all other hooks to obtain the report object
    outcome = yield
    rep = outcome.get_result()

    # set a report attribute for each phase of a call, which can
    # be "setup", "call", "teardown"

    setattr(item, "rep_" + rep.when, rep)


@pytest.fixture
def directory(request, test_base_dir, test_name):
    """Return a per-test specific directory.

    This makes a unique test-directory even if a test is rerun multiple times.

    """
    global ATTEMPTS
    # Auto set value if it isn't in the dict yet
    ATTEMPTS[test_name] = ATTEMPTS.get(test_name, 0) + 1
    directory = os.path.join(
        test_base_dir, "{}_{}".format(test_name, ATTEMPTS[test_name])
    )

    if not os.path.exists(directory):
        os.makedirs(directory)

    yield directory

    # test_base_dir is at the session scope, so we can't use request.node as mentioned in
    # the doc linked in the hook above.
    if request.session.testsfailed == 0:
        try:
            shutil.rmtree(directory)
        except Exception:
            files = [
                os.path.join(dp, f) for dp, _, fn in os.walk(directory) for f in fn
            ]
            print("Directory still contains files:", files)
            raise
    else:
        print(f"Test failed, leaving directory '{directory}' intact")


@pytest.fixture
def test_name(request):
    yield request.function.__name__


@pytest.fixture
def executor(test_name):
    ex = futures.ThreadPoolExecutor(
        max_workers=EXECUTOR_WORKERS, thread_name_prefix=test_name
    )
    yield ex
    ex.shutdown(wait=False)


@pytest.fixture
def bitcoind(directory):
    bitcoind = Bitcoind(bitcoin_dir=os.path.join(directory, "bitcoind"))
    bitcoind.startup()

    bitcoind.rpc.createwallet(
        bitcoind.rpc.wallet_name, False, False, "", False, True, True
    )

    bitcoind.rpc.generatetoaddress(101, bitcoind.rpc.getnewaddress())
    while bitcoind.rpc.getbalance() < 50:
        time.sleep(0.01)

    yield bitcoind

    bitcoind.cleanup()


def xpub_fingerprint(hd):
    return _pubkey_to_fingerprint(hd.pubkey).hex()


@pytest.fixture
def lianad(bitcoind, directory):
    datadir = os.path.join(directory, "lianad")
    os.makedirs(datadir, exist_ok=True)
    bitcoind_cookie = os.path.join(bitcoind.bitcoin_dir, "regtest", ".cookie")

    signer = SingleSigner()
    (prim_fingerprint, primary_xpub), (reco_fingerprint, recovery_xpub) = (
        (xpub_fingerprint(signer.primary_hd), signer.primary_hd.get_xpub()),
        (xpub_fingerprint(signer.recovery_hd), signer.recovery_hd.get_xpub()),
    )
    csv_value = 10
    # NOTE: origins are the actual xpub themselves which is incorrect but make it
    # possible to differentiate them.
    main_desc = Descriptor.from_str(
        f"wsh(or_d(pk([{prim_fingerprint}]{primary_xpub}/<0;1>/*),and_v(v:pkh([{reco_fingerprint}]{recovery_xpub}/<0;1>/*),older({csv_value}))))"
    )

    lianad = Lianad(
        datadir,
        signer,
        main_desc,
        bitcoind.rpcport,
        bitcoind_cookie,
    )

    try:
        lianad.start()
        yield lianad
    except Exception:
        lianad.cleanup()
        raise

    lianad.cleanup()


def multi_expression(thresh, keys):
    exp = f"multi({thresh},"
    for i, key in enumerate(keys):
        # NOTE: origins are the actual xpub themselves which is incorrect but make it
        # possible to differentiate them.
        fingerprint = xpub_fingerprint(key)
        exp += f"[{fingerprint}]{key.get_xpub()}/<0;1>/*"
        if i != len(keys) - 1:
            exp += ","
    return exp + ")"


@pytest.fixture
def lianad_multisig(bitcoind, directory):
    datadir = os.path.join(directory, "lianad")
    os.makedirs(datadir, exist_ok=True)
    bitcoind_cookie = os.path.join(bitcoind.bitcoin_dir, "regtest", ".cookie")

    # A 3-of-4 that degrades into a 2-of-5 after 10 blocks
    csv_value = 10
    signer = MultiSigner(4, {csv_value: 5})
    prim_multi, recov_multi = (
        multi_expression(3, signer.prim_hds),
        multi_expression(2, signer.recov_hds[csv_value]),
    )
    main_desc = Descriptor.from_str(
        f"wsh(or_d({prim_multi},and_v(v:{recov_multi},older({csv_value}))))"
    )

    lianad = Lianad(
        datadir,
        signer,
        main_desc,
        bitcoind.rpcport,
        bitcoind_cookie,
    )

    try:
        lianad.start()
        yield lianad
    except Exception:
        lianad.cleanup()
        raise

    lianad.cleanup()


@pytest.fixture
def lianad_multipath(bitcoind, directory):
    datadir = os.path.join(directory, "lianad")
    os.makedirs(datadir, exist_ok=True)
    bitcoind_cookie = os.path.join(bitcoind.bitcoin_dir, "regtest", ".cookie")

    # A 3-of-4 that degrades into a 3-of-5 after 10 blocks and into a 1-of-10 after 20 blocks.
    csv_values = [10, 20]
    signer = MultiSigner(4, {csv_values[0]: 5, csv_values[1]: 10})
    prim_multi = multi_expression(3, signer.prim_hds)
    first_recov_multi = multi_expression(3, signer.recov_hds[csv_values[0]])
    second_recov_multi = multi_expression(1, signer.recov_hds[csv_values[1]])
    main_desc = Descriptor.from_str(
        f"wsh(or_d({prim_multi},or_i(and_v(v:{first_recov_multi},older({csv_values[0]})),and_v(v:{second_recov_multi},older({csv_values[1]})))))"
    )

    lianad = Lianad(
        datadir,
        signer,
        main_desc,
        bitcoind.rpcport,
        bitcoind_cookie,
    )

    try:
        lianad.start()
        yield lianad
    except Exception:
        lianad.cleanup()
        raise

    lianad.cleanup()
