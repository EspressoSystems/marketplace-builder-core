searchState.loadedDescShard("marketplace_builder_core", 0, "Response Message to be put on the response channel\nBuilder State to hold the state of the builder\nDA Proposal Message to be put on the da proposal channel\nDecide Message to be put on the decide channel\nUnifies the possible messages that can be received by the …\nQC Message to be put on the quorum proposal channel\nRequest Message to be put on the request channel\nResponse Message to be put on the response channel\nEnum to hold the status out of the decide event\nEnum to hold the different sources of the transaction\nconstant fee that the builder will offer per byte of data …\nlocally spawned builder Commitements\nthe spawned from info for a builder state\nda_proposal_payload_commit to (da_proposal, node_count)\nda proposal receiver\ndecide receiver\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nglobal state handle, defined in the service.rs\ninstance state to enfoce max_block_size\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\ntimeout for maximising the txns in the block\ntotal nodes required for the VID computation as part of …\nquorum proposal receiver\nquorum_proposal_payload_commit to quorum_proposal\nchannel receiver for the block requests\nfiltered queue of available transactions, taken from …\nincoming stream of transactions\ntxns currently in the tx_queue\nvalidated state that is required for a proposal to be …\nThe channel is empty and closed.\nThe channel is empty and closed.\nThe channel is empty but not closed.\nThe channel has overflowed since the last element was …\nThe channel has overflowed since the last element was …\nAn error returned from <code>Receiver::recv()</code>.\nAn error returned from <code>Receiver::try_recv()</code>.\nCreate a new broadcast channel.\nTODO: fetch actual blocks\nFor the DA proposal.\nFor the DA proposal.\nFor the decide.\nFor the decide.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nFor the quorum proposal.\nFor the quorum proposal.\nFor transactions, shared.\nFor transactions, shared.\nBLS private key used to sign a message\nBLS public key used to verify a signature\nThe base version of HotShot this node is instantiated with.\nThe block header type that this hotshot setup is using.\nAbstraction over the full contents of a block\nThe block type that this hotshot setup is using.\nThe receiving side of a channel.\nThe sending side of the broadcast channel.\nThe type builder uses to sign its messages\nThe channel is empty and closed.\nThe channel is empty and closed.\nTrait for time compatibility needed for reward collection\nA proposal to start providing data availability for a …\nThe channel is empty but not closed.\nThe error type for this type of block\nDummy implementation of <code>Membership</code>\nThe type of the instance-level state this state is …\nThe instance-level state type that this hotshot setup is …\nThis is the consensus-internal analogous concept to a …\nMembership used for this implementation\nData created during block building which feeds into the …\nTrait with all the type definitions that are used in the …\nThe channel has overflowed since the last element was …\nThe channel has overflowed since the last element was …\nPrepare qc from the leader\nType alias for a <code>QuorumCertificate</code>, which is a …\nProposal to append a block.\nAn error returned from <code>Receiver::recv()</code>.\nThe signature key that this hotshot setup is using.\nA certificate which can be created by aggregating many …\nDefines a threshold which is 2f + 1 (Amount needed for …\nThe time type that this hotshot setup is using.\nThe transaction type that this hotshot setup is using.\nThe type of the transitions we are applying\nAn error returned from <code>Receiver::try_recv()</code>.\nThe hash for the upgrade.\nThe version of HotShot this node may be upgraded to. Set …\nThe validated state type that this hotshot setup is using.\nValidated State\nType-safe wrapper around <code>u64</code> so we know the thing we’re …\nphantom data for <code>THRESHOLD</code> and <code>TYPES</code>\nPhantom for TYPES\nphantom data for <code>THRESHOLD</code> and <code>TYPES</code>\nIf sender will wait for active receivers.\nIf sender will wait for active receivers.\nThe block header contained in this leaf.\nThe block header to append\nGet a mutable reference to the block header contained in …\nOptional block payload.\nCreate a new broadcast channel.\nBroadcasts a message on the channel.\nBroadcasts a message on the channel without pinning the …\nGenerate commitment that builders use to sign block …\nReturns the channel capacity.\nReturns the channel capacity.\nProduce a clone of this Receiver that has the same …\nCloses the channel.\nCloses the channel.\nClone the public key and corresponding stake table for …\nGet the network topic for the committee\nThe data this certificate is for.  I.e the thing that was …\nThe data being proposed.\nThe data this certificate is for.  I.e the thing that was …\nDowngrade to a <code>InactiveReceiver</code>.\nBuild the payload and metadata for genesis/null block.\nEncoded transactions in the block to be applied.\nValidate that a leaf has the right upgrade certificate to …\nFill this leaf with the block payload.\nFill this leaf with the block payload, without checking …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nBuild a payload with the encoded transaction bytes, …\nConstructs a leaf from a given quorum proposal.\nBuild a payload and associated metadata with the …\nCreate a new instance of this time unit at time number 0\nCreate a genesis view number (0)\nCreate a new leaf from its components.\nCreat the Genesis certificate\nHeight of this leaf in the chain.\nReturns the number of inactive receivers for the channel.\nReturns the number of inactive receivers for the channel.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nReturns <code>true</code> if the channel is closed.\nReturns <code>true</code> if the channel is closed.\nReturns <code>true</code> if the channel is empty and closed.\nReturns <code>true</code> if the channel is empty.\nReturns <code>true</code> if the channel is empty.\nReturns <code>true</code> if the channel is empty but not closed.\nReturns <code>true</code> if the channel is full.\nReturns <code>true</code> if the channel is full.\nReturns <code>true</code> if this error indicates the receiver missed …\nDetermines whether or not a certificate is relevant (i.e. …\nThe QC linking this leaf to its parent in the chain.\nPer spec, justification\nIndex the vector of public keys with the current view …\nReturns the number of messages in the channel.\nReturns the number of messages in the channel.\nMetadata of the block to be applied.\nCreate a new instance of this time unit\nCreates a new dummy elector\nCreate a new <code>ViewNumber</code> with the given value.\nProduce a new Receiver for this channel.\nProduce a new Receiver for this channel.\nProduce a new Sender for this channel.\nget all the non-staked nodes\nget the non-staked builder nodes\nNumber of transactions in the block.\nIf overflow mode is enabled on this channel.\nIf overflow mode is enabled on this channel.\nCommitment to this leaf’s parent.\nA commitment to the block payload contained in this leaf.\nA low level poll method that is similar to <code>Receiver::recv()</code>…\nPossible timeout or view sync certificate.\nReturns the number of receivers for the channel.\nReturns the number of receivers for the channel.\nReceives a message from the channel.\nReceives a message from the channel without pinning the …\nReturns the number of senders for the channel.\nReturns the number of senders for the channel.\nSpecify if sender will wait for active receivers.\nSpecify if sender will wait for active receivers.\nSet the channel capacity.\nSet the channel capacity.\nSet overflow mode on the channel.\nSet overflow mode on the channel.\nThe proposal must be signed by the view leader\nassembled signature for certificate aggregation\nassembled signature for certificate aggregation\nValidate that a leaf has the right upgrade certificate to …\nDetermines whether or not a certificate is relevant (i.e. …\nList of transaction commitments.\nGet the transactions in the payload.\nAttempts to broadcast a message on the channel.\nAttempts to receive a message from the channel.\nGet the u64 format of time\nReturen the u64 format\nThe QC linking this leaf to its parent in the chain.\nPossible upgrade certificate, which the leader may …\nGiven an upgrade certificate and a view, tests whether the …\nValidate an upgrade certificate.\nChecks that the signature of the quorum proposal is valid.\nTime when this leaf was created.\nWhich view this QC relates to\nView this proposal applies to\nCurView from leader when proposing leaf\nWhich view this QC relates to\ncommitment of all the votes this cert should be signed over\ncommitment of all the votes this cert should be signed over\nA set that allows for time-based garbage collection, …\nReturns <code>true</code> if the key is contained in the set\nForce garbage collection, even if the time elapsed since …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nInsert a <code>key</code> into the set. Doesn’t trigger garbage …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nConstruct a new <code>RotatingSet</code>\nTrigger garbage collection.")