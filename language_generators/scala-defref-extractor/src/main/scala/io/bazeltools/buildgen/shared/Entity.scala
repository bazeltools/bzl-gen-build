package io.bazeltools.buildgen.shared

import cats.data.{Chain, NonEmptyList}
import cats.Order
import cats.parse.{Parser, Parser0}
import io.circe.{Decoder, Encoder}

import cats.syntax.all._

final case class Entity(parts: Vector[String]) {
  def init: Entity = Entity(parts.init)

  def prefixes: NonEmptyList[Entity] =
    // inits always returns an nonempty list
    NonEmptyList.fromListUnsafe(parts.inits.toList.map(Entity(_)))

  def resolve(fn: String => Entity.Resolved): Entity.Resolved =
    fn(parts.head) / Entity(parts.tail)

  def isSingleton: Boolean = parts.lengthCompare(1) == 0

  def startsWith(that: Entity): Boolean =
    (that.parts.length <= parts.length) &&
      parts.iterator.zip(that.parts.iterator).forall { case (a, b) => a == b }

  def asString: String = parts.mkString(".")

  def /(that: String): Entity = {
    require(that.nonEmpty)
    Entity(parts :+ that)
  }

  def /(that: Entity): Entity =
    Entity(parts ++ that.parts)

  override def toString(): String = asString
}

object Entity {

  implicit val entityEncoder: Encoder[Entity] =
    Encoder.encodeString.contramap(_.asString)

  implicit val entityDecoder: Decoder[Entity] =
    Decoder.decodeString.map(dotted(_))

  def makeSpecialTldsMap(names: Iterable[String]): Map[String, Entity.Resolved] =
    names.iterator.map { name =>
      (name, Entity.Resolved.Known(Entity.simple(name)))
    }.toMap

  val empty: Entity = Entity(Vector.empty)

  def simple(s: String): Entity = {
    require(s.nonEmpty)
    Entity(Vector(s))
  }

  def dotted(s: String): Entity =
    Entity(s.split("\\.", -1).toVector)

  implicit val catsOrderEntity: Order[Entity] =
    Order[Vector[String]].contramap[Entity](_.parts)

  implicit val entityOrdering: Ordering[Entity] =
    catsOrderEntity.toOrdering

  sealed abstract class Resolved {
    def /(that: String): Resolved

    def /(that: Entity): Resolved =
      that.parts.foldLeft(this)(_ / _)

    def |(that: Resolved): Resolved =
      Resolved.Many(this, that)

    // These are all the non-local entities
    def entities: Chain[Entity]
  }
  object Resolved {
    sealed abstract class Definite extends Resolved
    case class Imported(original: Resolved) extends Definite {
      def /(that: String) = Imported(original / that)

      def entities = original.entities
    }
    case class Local(scopePath: List[Long], entity: Entity) extends Definite {
      def /(that: String) = Local(scopePath, entity / that)
      def entities = Chain.empty
    }
    case class Known(name: Entity) extends Definite {
      def /(that: String) = Known(name / that)
      def entities = Chain.one(name)
    }
    case class Many(left: Resolved, right: Resolved) extends Resolved {
      def /(that: String) = Many(left / that, right / that)

      def entities = left.entities ++ right.entities
    }
  }

  object Parsers {
    def notThen[A](n: Parser0[Any], t: Parser[A]): Parser[A] =
      (!n).with1 *> t

    // extract all the items we can find by ignoreing
    // characters that start at an epsilon failure
    def repSkip[A](item: Parser[A]): Parser0[List[A]] = {
      val notItem = Parsers.notThen(item, Parser.anyChar).rep

      item.repSep0(notItem).surroundedBy(notItem.?)
    }
  }

  lazy val parseDirectives: Parser0[List[String]] = {
    // bzl_gen_build:dir:entity
    // Alternative here, for other name we've used
    val spaces0 = Parser.charIn(" \t").rep0
    val prefix = Parser.string("bzl_gen_build")
    val bzlBuildGen =
      (prefix.surroundedBy(spaces0) *> Parser.string(":")).surroundedBy(spaces0)

    val newLine = Parser.charIn("\r\n").void

    Parsers.repSkip(
      bzlBuildGen *> Parsers.notThen(newLine, Parser.anyChar).rep.string
    )
  }

  def findDirectives(str: String): Either[String, Chain[String]] = {
    parseDirectives
      .parseAll(str)
      .map(Chain.fromSeq(_))
      .leftMap(_.toString)
  }
}
