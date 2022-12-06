package io.bazeltools.buildgen.shared

import scala.collection.immutable.SortedSet
import cats.kernel.Monoid
import io.circe.Encoder
import io.circe.generic.semiauto.deriveEncoder


case class ExtractedData(
    data_blocks: List[DataBlock],
    label_or_repo_path: String
)

object ExtractedData {
    implicit val extractedDataEncoder: Encoder[ExtractedData] = deriveEncoder[ExtractedData]
}

final case class DataBlock(
      defs: SortedSet[Entity],
      refs: SortedSet[Entity],
      bzl_gen_build_commands: SortedSet[String] = SortedSet.empty
  ) {
    def addDef(e: Entity) = copy(defs = defs + e)
    def addRef(e: Entity) = copy(refs = refs + e)
    def addBzlBuildGenCommand(e: String) = copy(bzl_gen_build_commands = bzl_gen_build_commands + e)
    def addBzlBuildGenCommands(e: Iterable[String]) = copy(bzl_gen_build_commands = bzl_gen_build_commands ++ e)
  }



  object DataBlock {
    implicit val dataBlockEncoder: Encoder[DataBlock] = deriveEncoder[DataBlock]

    val empty: DataBlock = DataBlock(SortedSet.empty, SortedSet.empty)

    implicit val DataBlockMonoid: Monoid[DataBlock] =
      new Monoid[DataBlock] {
        def empty = DataBlock.empty
        def combine(left: DataBlock, right: DataBlock) =
          DataBlock(
            left.defs | right.defs,
            left.refs | right.refs,
            left.bzl_gen_build_commands | right.bzl_gen_build_commands
          )
      }
  }